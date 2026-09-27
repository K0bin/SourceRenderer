use std::marker::PhantomData;

use crate::{WebGPUBackend, WebGPUDevice, WebGPUSurface};
use sourcerenderer_core::gpu;
use web_sys::{GpuAdapter, GpuDevice};

pub struct WebGPUAdapter {
    _adapter: GpuAdapter,
    device: GpuDevice,
    debug: bool,
    adapter_type: gpu::AdapterType,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebGPUAdapter {
    pub fn new(
        adapter: GpuAdapter,
        device: GpuDevice,
        adapter_type: gpu::AdapterType,
        debug: bool,
    ) -> Self {
        Self {
            _adapter: adapter,
            device,
            debug,
            adapter_type,
            _p: PhantomData,
        }
    }
}

impl gpu::Adapter<WebGPUBackend> for WebGPUAdapter {
    fn adapter_type(&self) -> sourcerenderer_core::gpu::AdapterType {
        self.adapter_type
    }

    unsafe fn create_device(&self, _surface: &WebGPUSurface) -> WebGPUDevice {
        WebGPUDevice::new(self.device.clone(), self.debug)
    }
}
