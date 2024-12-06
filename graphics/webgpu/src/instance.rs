use log::error;
use sourcerenderer_core::gpu::Instance;
use web_sys::{GpuAdapter, Navigator};
use wasm_bindgen_futures::*;

use crate::{adapter::WebGPUAdapter, WebGPUBackend};

pub struct WebGPUInstance {
    adapter: [WebGPUAdapter; 1]
}

impl WebGPUInstance {
    pub async fn new(navigator: Navigator) -> Result<WebGPUInstance, ()> {
        let gpu = navigator.gpu();
        if !gpu.is_object() {
            error!("Browser does not support WebGPU");
            return Err(());
        }
        let adapter_future = JsFuture::from(gpu.request_adapter());
        let adapter: GpuAdapter = adapter_future
            .await
            .map_err(|_| ())?
            .into();

        Ok(Self {
            adapter: [WebGPUAdapter::new(adapter)]
        })
    }
}

impl Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[WebGPUAdapter] {
        &self.adapter
    }
}
