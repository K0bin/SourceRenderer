
use std::sync::Arc;

use metal::{self, MetalDrawable};
use block::ConcreteBlock;

use sourcerenderer_core::gpu::{self, CommandBuffer, Swapchain};

use super::*;

pub struct MTLQueue {
    queue: metal::CommandQueue,
    meta_shaders: Arc<MTLMetaShaders>
}

impl MTLQueue {
    pub(crate) fn new(device: &metal::DeviceRef, meta_shaders: &Arc<MTLMetaShaders>) -> Self {
        let queue = device.new_command_queue();
        Self {
            queue,
            meta_shaders: meta_shaders.clone()
        }

    }

    pub(crate) fn handle(&self) -> &metal::CommandQueueRef {
        &self.queue
    }
}

impl gpu::Queue<MTLBackend> for MTLQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, _flags: gpu::CommandPoolFlags) -> MTLCommandPool {
        MTLCommandPool::new(&self.queue, command_pool_type, &self.meta_shaders)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<MTLBackend>]) {
        for submission in submissions {
            for cmd_buf in submission.command_buffers {
                // We cannot add a wait for an event after encoding the command buffer, so each command buffer starts off with
                // a wait for its own event and we record a helper command buffer that does nothing but signal that event after waiting
                // for all events that are passed to the submission
                let fence_wait_cmd_buffer = self.queue.new_command_buffer_with_unretained_references();
                for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.wait_fences {
                    fence_wait_cmd_buffer.encode_wait_for_event(fence.event_handle(), *value);
                }
                fence_wait_cmd_buffer.encode_signal_event(cmd_buf.pre_event_handle(), 1);
                fence_wait_cmd_buffer.commit();

                // Fences are only supposed to be signalled after all command buffers in the submission are completed.
                // So if we have more than 1 command buffer, we need another helper command buffer that waits for each command buffer
                // and then does the actual signalling
                if submission.command_buffers.len() == 1 {
                    for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.signal_fences {
                        cmd_buf.handle().encode_signal_event(fence.event_handle(), *value);
                    }
                } else {
                    cmd_buf.handle().encode_signal_event(cmd_buf.post_event_handle(), 1);
                }

                cmd_buf.handle().commit();
            }
            if !submission.signal_fences.is_empty() && submission.command_buffers.len() > 1 {
                let fence_signal_cmd_buffer = self.queue.new_command_buffer_with_unretained_references();
                for cmd_buf in submission.command_buffers {
                    fence_signal_cmd_buffer.encode_wait_for_event(cmd_buf.post_event_handle(), 1);
                }
                for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.signal_fences {
                    fence_signal_cmd_buffer.encode_signal_event(fence.event_handle(), *value);
                }
                fence_signal_cmd_buffer.commit();
            }
        }
    }

    unsafe fn present(&self, swapchain: &MTLSwapchain) {
        let drawable = swapchain.take_drawable();
        let backbuffer = swapchain.backbuffer(swapchain.backbuffer_index());

        let drawable_mtl_texture = drawable.texture();
        let dst = MTLTexture::from_mtl_texture(drawable_mtl_texture, false);
        let mut cmd_buffer = MTLCommandBuffer::new(&self.queue, self.queue.new_command_buffer().to_owned(), &self.meta_shaders);
        // Begin/End are not actually necessary
        cmd_buffer.blit(backbuffer, 0, 0, &dst, 0, 0);
        cmd_buffer.handle().present_drawable(&drawable);
        cmd_buffer.handle().commit();
    }
}
