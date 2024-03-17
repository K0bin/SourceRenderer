use metal;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLQueue {
    queue: metal::CommandQueue
}

impl MTLQueue {
    pub(crate) fn new(device: &metal::Device) -> Self {
        let queue = device.new_command_queue();
        Self {
            queue
        }
    }

    pub(crate) fn handle(&self) -> &metal::CommandQueue {
        &self.queue
    }
}

impl gpu::Queue<MTLBackend> for MTLQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, flags: gpu::CommandPoolFlags) -> <MTLBackend as gpu::GPUBackend>::CommandPool {
        todo!()
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<MTLBackend>]) {
        todo!()
    }

    unsafe fn present(&self, swapchain: &MTLSwapchain) {
        todo!()
    }
}
