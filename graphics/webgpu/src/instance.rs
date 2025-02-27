use std::{error::Error, fmt::{Debug, Display}};

use sourcerenderer_core::gpu;
use web_sys::{GpuAdapter, GpuDevice, Navigator, GpuRequestAdapterOptions, GpuPowerPreference, Gpu};
use wasm_bindgen_futures::*;

use crate::{adapter::WebGPUAdapter, WebGPUBackend};

pub struct WebGPUInstanceAsyncInitResult {
    instance: Gpu,
    discrete_adapter: GpuAdapter,
    discrete_device: GpuDevice,
    integrated_adapter: GpuAdapter,
    integrated_device: GpuDevice
}

#[derive(Clone)]
pub struct WebGPUInstanceInitError {
    msg: String
}

impl WebGPUInstanceInitError {
    fn new(msg: &str) -> Self { Self { msg: msg.to_string() }}
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
    adapters: [WebGPUAdapter; 2]
}

unsafe impl Send for WebGPUInstance {}
unsafe impl Sync for WebGPUInstance {}

impl WebGPUInstance {
    pub async fn async_init(navigator: Navigator) -> Result<WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError> {
        let gpu = navigator.gpu();
        if !gpu.is_object() || gpu.is_null() || gpu.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Browser does not support WebGPU"));
        }
        let adapter_options = GpuRequestAdapterOptions::new();
        adapter_options.set_feature_level("core");
        adapter_options.set_power_preference(GpuPowerPreference::HighPerformance);
        let discrete_adapter_future = JsFuture::from(gpu.request_adapter_with_options(&adapter_options));
        let discrete_adapter: GpuAdapter = discrete_adapter_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"))?
            .into();

        if !discrete_adapter.is_object() || discrete_adapter.is_null() || discrete_adapter.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"));
        }

        let discrete_device_future = JsFuture::from(discrete_adapter.request_device());
        let discrete_device: GpuDevice = discrete_device_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"))?
            .into();

        if !discrete_device.is_object() || discrete_device.is_null() || discrete_device.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"));
        }

        adapter_options.set_power_preference(GpuPowerPreference::LowPower);
        let integrated_adapter_future = JsFuture::from(gpu.request_adapter_with_options(&adapter_options));
        let integrated_adapter: GpuAdapter = integrated_adapter_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"))?
            .into();

        if !integrated_adapter.is_object() || integrated_adapter.is_null() || integrated_adapter.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"));
        }

        let integrated_device_future = JsFuture::from(integrated_adapter.request_device());
        let integrated_device: GpuDevice = integrated_device_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"))?
            .into();

        if !integrated_device.is_object() || integrated_device.is_null() || integrated_device.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"));
        }

        Ok(WebGPUInstanceAsyncInitResult {
            instance: gpu,
            discrete_adapter,
            discrete_device,
            integrated_adapter,
            integrated_device
        })
    }

    pub fn new(async_result: &WebGPUInstanceAsyncInitResult, debug: bool) -> Self {
        Self {
            instance: async_result.instance.clone(),
            adapters: [
                WebGPUAdapter::new(
                    async_result.discrete_adapter.clone(),
                    async_result.discrete_device.clone(),
                    gpu::AdapterType::Discrete,
                    debug
                ),
                WebGPUAdapter::new(
                    async_result.integrated_adapter.clone(),
                    async_result.integrated_device.clone(),
                    gpu::AdapterType::Integrated,
                    debug
                )
            ]
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &Gpu {
        &self.instance
    }
}

impl gpu::Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[WebGPUAdapter] {
        &self.adapters
    }
}
