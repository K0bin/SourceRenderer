use crate::{WebGPUBackend, adapter::WebGPUAdapter};
use js_sys::wasm_bindgen::JsCast;
use js_sys::{JsNullable, JsString};
use smallvec::SmallVec;
use sourcerenderer_core::gpu;
use std::marker::PhantomData;
use std::{
    error::Error,
    fmt::{Debug, Display},
};
use wasm_bindgen_futures::*;
use web_sys::{
    DedicatedWorkerGlobalScope, Gpu, GpuAdapter, GpuDevice, GpuDeviceDescriptor,
    GpuPowerPreference, GpuRequestAdapterOptions, window,
};

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
    pub async fn new(debug: bool) -> Result<Self, WebGPUInstanceInitError> {
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
        Ok(Self {
            instance: gpu,
            adapters: Some([
                WebGPUAdapter::new(
                    discrete_adapter.clone(),
                    discrete_device.clone(),
                    gpu::AdapterType::Discrete,
                    debug,
                ),
                WebGPUAdapter::new(
                    integrated_adapter.clone(),
                    integrated_device.clone(),
                    gpu::AdapterType::Integrated,
                    debug,
                ),
            ]),
            _p: PhantomData,
        })
    }

    pub fn new_dummy() -> Self {
        Self {
            instance: Self::get_webgpu(),
            adapters: None,
            _p: PhantomData,
        }
    }

    pub(crate) fn get_webgpu() -> Gpu {
        window().map_or_else(
            || {
                let global = js_sys::global();
                let worker_scope: DedicatedWorkerGlobalScope = global.dyn_into().unwrap();
                worker_scope.navigator().gpu()
            },
            |window| window.navigator().gpu(),
        )
    }

    #[inline(always)]
    pub fn handle(&self) -> &Gpu {
        &self.instance
    }
}

impl gpu::Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[WebGPUAdapter] {
        self.adapters
            .as_ref()
            .expect("Can't create adapters from a dummy instance.")
    }
}
