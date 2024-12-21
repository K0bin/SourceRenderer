use std::sync::atomic::{AtomicU64, Ordering};

use js_sys::Array;
use sourcerenderer_core::gpu;
use web_sys::{GpuDevice, GpuQueue};

use crate::{command::WebGPUCommandPool, swapchain::WebGPUSwapchain, WebGPUBackend};


pub struct WebGPUQueue {
    device: GpuDevice,
    queue: GpuQueue,
}

impl WebGPUQueue {
    pub fn new(device: &GpuDevice) -> Self {
        let queue = device.queue();
        Self {
            device: device.clone(),
            queue,
        }
    }
}

unsafe impl Send for WebGPUQueue {}
unsafe impl Sync for WebGPUQueue {}

impl gpu::Queue<WebGPUBackend> for WebGPUQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, _flags: gpu::CommandPoolFlags) -> WebGPUCommandPool {
        WebGPUCommandPool::new(&self.device, command_pool_type)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<WebGPUBackend>]) {
        for submission in submissions {
            let is_ready = submission.wait_fences.iter().all(|pair| pair.fence.value.load(Ordering::Release) >= pair.value);
            assert!(is_ready);

            let array = Array::new_with_length(submission.command_buffers.len() as u32);
            for (index, cmd_buffer) in submission.command_buffers.iter().enumerate() {
                array.set(index as u32, cmd_buffer.handle().into());
            }
            self.queue.submit(&array);
            for pair in submission.signal_fences {
                if pair.fence.value.load(Ordering::Acquire) < pair.value {
                    pair.fence.value.store(pair.value, Ordering::Release);
                }
            }
        }
    }

    unsafe fn present(&self, _swapchain: &WebGPUSwapchain) {}
}

pub struct WebGPUFence {
    value: AtomicU64,
}

impl gpu::Fence for WebGPUFence {
    unsafe fn value(&self) -> u64 {
        self.value.load(Ordering::SeqCst)
    }

    unsafe fn await_value(&self, _value: u64) {}
}
