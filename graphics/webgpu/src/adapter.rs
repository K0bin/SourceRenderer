use sourcerenderer_core::gpu::{Adapter, AdapterType};
use web_sys::{GpuAdapter, GpuDevice};

use crate::{WebGPUBackend, WebGPUDevice};

pub struct WebGPUAdapter {
    adapter: GpuAdapter,
    device: GpuDevice
}

impl WebGPUAdapter {
    pub fn new(adapter: GpuAdapter, device: GpuDevice) -> Self {
        Self {
            adapter,
            device,
        }
    }
}

unsafe impl Send for WebGPUAdapter {}
unsafe impl Sync for WebGPUAdapter {}

impl Adapter<WebGPUBackend> for WebGPUAdapter {
    fn adapter_type(&self) -> sourcerenderer_core::gpu::AdapterType {
        AdapterType::Other
    }

    fn create_device(&self, _surface: &<WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Surface) -> WebGPUDevice {
        WebGPUDevice::new(self.device.clone())
    }
}