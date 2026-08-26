use crate::{WebGPUBackend, adapter::WebGPUAdapter};
use js_sys::wasm_bindgen::JsCast;
use js_sys::{global, JsNullable, JsString};
use smallvec::SmallVec;
use sourcerenderer_core::gpu;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::{
    error::Error,
    fmt::{Debug, Display},
};
use wasm_bindgen_futures::*;
use web_sys::{DedicatedWorkerGlobalScope, Gpu, GpuAdapter, GpuDevice, GpuDeviceDescriptor, GpuPowerPreference, GpuRequestAdapterOptions, window};

thread_local! {
    static GPU_INIT: RefCell<Option<WebGPUInstanceAsyncInitResult>> = RefCell::new(None);
}

struct WebGPUInstanceAsyncInitResult {
    instance: Gpu,
    discrete_adapter: GpuAdapter,
    discrete_device: GpuDevice,
    integrated_adapter: GpuAdapter,
    integrated_device: GpuDevice,
    _p: PhantomData<*const std::ffi::c_void>,
}

#[derive(Clone)]
pub struct WebGPUInstanceInitError {
    msg: String,
}

impl WebGPUInstanceInitError {
    fn new(msg: &str) -> Self {
        Self {
            msg: msg.to_string(),
        }
    }

    pub fn uninited() -> Self {
        Self::new("The asynchronous WebGPU process has not yet been started.")
    }
    pub fn unfinished() -> Self {
        Self::new("The asynchronous WebGPU initialization has not yet finished.")
    }
    pub fn cleared() -> Self {
        Self::new("The WebGPU cache has been cleared.")
    }
}

impl Display for WebGPUInstanceInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl Debug for WebGPUInstanceInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

impl Error for WebGPUInstanceInitError {}

pub struct WebGPUInstance {
    instance: Gpu,
    adapters: Option<[WebGPUAdapter; 2]>,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebGPUInstance {
    pub async fn async_init() -> Result<(), WebGPUInstanceInitError> {
        let gpu = Self::get_webgpu();
        if !gpu.is_object() || gpu.is_null() || gpu.is_undefined() {
            return Err(WebGPUInstanceInitError::new(
                "Browser does not support WebGPU",
            ));
        }
        let adapter_options = GpuRequestAdapterOptions::new();
        adapter_options.set_feature_level("core");
        adapter_options.set_power_preference(GpuPowerPreference::HighPerformance);
        let discrete_adapter_future =
            JsFuture::from(gpu.request_adapter_with_options(&adapter_options));
        let nullable_discrete_adapter: JsNullable<GpuAdapter> = discrete_adapter_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"))?
            .into();

        if nullable_discrete_adapter.is_empty() {
            return Err(WebGPUInstanceInitError::new(
                "Failed to retrieve WebGPU adapter",
            ));
        }
        let discrete_adapter: GpuAdapter = nullable_discrete_adapter.unwrap();
        let discrete_device_descriptor = GpuDeviceDescriptor::new();
        let mut discrete_device_features = SmallVec::<[JsString; 4]>::new();
        for feature_res in discrete_adapter.features().values() {
            // TODO: Filter out a bunch that we never use.
            let feature = feature_res.unwrap();
            discrete_device_features.push(feature);
        }
        discrete_device_descriptor.set_required_features(&discrete_device_features);

        let discrete_device_future = JsFuture::from(
            discrete_adapter.request_device_with_descriptor(&discrete_device_descriptor),
        );
        let discrete_device: GpuDevice = discrete_device_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"))?
            .into();

        if !discrete_device.is_object()
            || discrete_device.is_null()
            || discrete_device.is_undefined()
        {
            return Err(WebGPUInstanceInitError::new(
                "Failed to retrieve WebGPU device",
            ));
        }

        adapter_options.set_power_preference(GpuPowerPreference::LowPower);
        let integrated_adapter_future =
            JsFuture::from(gpu.request_adapter_with_options(&adapter_options));

        let nullable_integrated_adapter: JsNullable<GpuAdapter> = integrated_adapter_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"))?
            .into();

        if nullable_integrated_adapter.is_empty() {
            return Err(WebGPUInstanceInitError::new(
                "Failed to retrieve WebGPU adapter",
            ));
        }

        let integrated_adapter = nullable_integrated_adapter.unwrap();
        let integrated_device_descriptor = GpuDeviceDescriptor::new();
        let mut integrated_device_features = SmallVec::<[JsString; 4]>::new();
        for feature_res in integrated_adapter.features().values() {
            // TODO: Filter out a bunch that we never use.
            let feature = feature_res.unwrap();
            integrated_device_features.push(feature);
        }
        integrated_device_descriptor.set_required_features(&integrated_device_features);

        let integrated_device_future = JsFuture::from(
            integrated_adapter.request_device_with_descriptor(&integrated_device_descriptor),
        );
        let integrated_device: GpuDevice = integrated_device_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"))?
            .into();

        if !integrated_device.is_object()
            || integrated_device.is_null()
            || integrated_device.is_undefined()
        {
            return Err(WebGPUInstanceInitError::new(
                "Failed to retrieve WebGPU device",
            ));
        }

        GPU_INIT.replace(Some(WebGPUInstanceAsyncInitResult {
            instance: gpu,
            discrete_adapter,
            discrete_device,
            integrated_adapter,
            integrated_device,
            _p: PhantomData,
        }));

        Ok(())
    }

    pub fn clear_thread_cache() {
        GPU_INIT.replace(None);
    }

    pub fn new(debug: bool) -> Self {
        GPU_INIT.with_borrow(|init_ref_cell| {
            init_ref_cell.as_ref().map_or_else(|| Self::new_dummy(), |init|
            Self {
                instance: init.instance.clone(),
                adapters: Some([
                    WebGPUAdapter::new(
                        init.discrete_adapter.clone(),
                        init.discrete_device.clone(),
                        gpu::AdapterType::Discrete,
                        debug,
                    ),
                    WebGPUAdapter::new(
                        init.integrated_adapter.clone(),
                        init.integrated_device.clone(),
                        gpu::AdapterType::Integrated,
                        debug,
                    ),
                ]),
                _p: PhantomData,
            }
        )})
    }

    fn new_dummy() -> Self {
        Self {
            instance: Self::get_webgpu(),
            adapters: None,
            _p: PhantomData,
        }
    }

    pub(crate) fn get_webgpu() -> Gpu {
        global().dyn_into::<DedicatedWorkerGlobalScope>().map_or_else(
            |_e| {
                window().unwrap().navigator().gpu()
            },
            |scope| {
                scope.navigator().gpu()
            },
        )
    }

    #[inline(always)]
    pub fn handle(&self) -> &Gpu {
        &self.instance
    }
}

impl gpu::Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[WebGPUAdapter] {
        self.adapters.as_ref().map(|a| &a[..]).unwrap_or(&[])
    }
}
