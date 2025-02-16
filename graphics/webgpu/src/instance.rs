use std::{error::Error, fmt::{Debug, Display}};

use log::{error, warn};
use sourcerenderer_core::gpu::Instance;
use web_sys::{GpuAdapter, GpuDevice, Navigator};
use wasm_bindgen_futures::*;

use crate::{adapter::WebGPUAdapter, WebGPUBackend};

pub struct WebGPUInstanceAsyncInitResult {
    adapter: GpuAdapter,
    device: GpuDevice
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
    adapters: [WebGPUAdapter; 1]
}

impl WebGPUInstance {
    pub async fn async_init(navigator: Navigator) -> Result<WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError> {
        let gpu = navigator.gpu();
        if !gpu.is_object() || gpu.is_null() || gpu.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Browser does not support WebGPU"));
        }
        let adapter_future = JsFuture::from(gpu.request_adapter());
        let adapter: GpuAdapter = adapter_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"))?
            .into();

        if !adapter.is_object() || adapter.is_null() || adapter.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU adapter"));
        }

        let device_future = JsFuture::from(adapter.request_device());
        let device: GpuDevice = device_future
            .await
            .map_err(|_| WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"))?
            .into();

        if !device.is_object() || device.is_null() || device.is_undefined() {
            return Err(WebGPUInstanceInitError::new("Failed to retrieve WebGPU device"));
        }

        Ok(WebGPUInstanceAsyncInitResult {
            adapter,
            device
        })
    }

    pub fn new(async_result: &WebGPUInstanceAsyncInitResult, debug: bool) -> Self {
        Self {
            adapters: [
                WebGPUAdapter::new(
                    async_result.adapter.clone(),
                    async_result.device.clone(),
                    debug
                )
            ]
        }
    }

    pub fn device(&self) -> &GpuDevice {
        self.adapters[0].device()
    }
}

impl Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[WebGPUAdapter] {
        &self.adapters
    }
}
