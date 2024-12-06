use sourcerenderer_core::gpu::{Adapter, AdapterType};
use web_sys::GpuAdapter;

use crate::WebGPUBackend;

pub struct WebGPUAdapter {
    adapter: GpuAdapter
}

impl WebGPUAdapter {
    pub fn new(adapter: GpuAdapter) -> Self {
        Self {
            adapter
        }
    }
}

unsafe impl Send for WebGPUAdapter {}
unsafe impl Sync for WebGPUAdapter {}

impl Adapter<WebGPUBackend> for WebGPUAdapter {
    fn adapter_type(&self) -> sourcerenderer_core::gpu::AdapterType {
        AdapterType::Other
    }

    fn create_device(&self, surface: &<WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Surface) -> <WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Device {
        todo!()
    }
}