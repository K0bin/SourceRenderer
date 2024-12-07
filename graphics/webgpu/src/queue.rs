use sourcerenderer_core::gpu;
use web_sys::{GpuDevice, GpuQueue};

use crate::WebGPUBackend;

pub struct WebGPUQueue {
    queue: GpuQueue
}

impl WebGPUQueue {
    pub fn new(device: &GpuDevice) -> Self {
        let queue = device.queue();
        Self {
            queue
        }
    }
}

unsafe impl Send for WebGPUQueue {}
unsafe impl Sync for WebGPUQueue {}

impl gpu::Queue<WebGPUBackend> for WebGPUQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, flags: gpu::CommandPoolFlags) -> B::CommandPool {
        todo!()
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<B>]) {
        todo!()
    }

    unsafe fn present(&self, swapchain: &B::Swapchain) {
        todo!()
    }
}


pub struct WebGPUFence {}

impl gpu::Fence for WebGPUFence {
    unsafe fn value(&self) -> u64 {
        todo!()
    }

    unsafe fn await_value(&self, value: u64) {
        todo!()
    }
}
