use std::sync::atomic::{AtomicU64, Ordering};

use js_sys::Array;
use sourcerenderer_core::gpu;
use web_sys::{GpuDevice, GpuQueue};

use crate::{
    swapchain::WebGPUSwapchain, WebGPUBackbuffer, WebGPUBackend, WebGPUCommandPool, WebGPULimits,
};

pub struct WebGPUQueue {
    device: GpuDevice,
    queue: GpuQueue,
    limits: WebGPULimits,
}

impl WebGPUQueue {
    pub(crate) fn new(device: &GpuDevice, limits: &WebGPULimits) -> Self {
        let queue = device.queue();
        Self {
            device: device.clone(),
            queue,
            limits: limits.clone(),
        }
    }

    pub(crate) fn handle(&self) -> &GpuQueue {
        &self.queue
    }
}

impl gpu::Queue<WebGPUBackend> for WebGPUQueue {
    unsafe fn create_command_pool(
        &self,
        command_pool_type: gpu::CommandPoolType,
        _flags: gpu::CommandPoolFlags,
    ) -> WebGPUCommandPool {
        WebGPUCommandPool::new(&self.device, command_pool_type, &self.limits)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<WebGPUBackend>]) {
        for submission in submissions {
            let is_ready = submission
                .wait_fences
                .iter()
                .all(|pair| pair.fence.value.load(Ordering::Acquire) >= pair.value);
            assert!(is_ready);

            let array = Array::new_with_length(submission.command_buffers.len() as u32);
            for (index, cmd_buffer) in submission.command_buffers.iter().enumerate() {
                // Unmap all readback buffers that get used in this command buffer to make them accessible on the GPU
                for sync in cmd_buffer.readback_syncs() {
                    if let Some(dst) = sync.dst.as_ref() {
                        dst.unmap();
                    } else {
                        sync.src.unmap();
                    }
                }

                array.set(index as u32, cmd_buffer.handle().into());
            }
            self.queue.submit(&array);
            for pair in submission.signal_fences {
                if pair.fence.value.load(Ordering::Acquire) < pair.value {
                    pair.fence.value.store(pair.value, Ordering::Release);
                }
            }
            for cmd_buffer in submission.command_buffers {
                // Map all readback buffers that get used in this command buffer to make them accessible on the CPU.
                // Hopefully the async tasks finishes early enough for them to be available by the time WebGPUBuffer::map() gets called.
                for sync in cmd_buffer.readback_syncs() {
                    if let Some(dst) = sync.dst.as_ref() {
                        let _ = dst.map_async(web_sys::gpu_map_mode::READ);
                    } else {
                        let _ = sync.src.map_async(web_sys::gpu_map_mode::READ);
                    }
                }
            }
        }
    }

    unsafe fn present(&self, _swapchain: &mut WebGPUSwapchain, _backbuffer: &WebGPUBackbuffer) {}
}

pub struct WebGPUFence {
    value: AtomicU64,
}

impl WebGPUFence {
    pub(crate) fn new(_gpu: &GpuDevice) -> Self {
        Self {
            value: AtomicU64::new(0u64),
        }
    }
}

impl gpu::Fence for WebGPUFence {
    unsafe fn value(&self) -> u64 {
        self.value.load(Ordering::SeqCst)
    }

    unsafe fn await_value(&self, _value: u64) {}
}
