
use metal;
use block::ConcreteBlock;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLQueue {
    queue: metal::CommandQueue,
}

impl MTLQueue {
    pub(crate) fn new(device: &metal::DeviceRef) -> Self {
        let queue = device.new_command_queue();
        Self {
            queue
        }

    }

    pub(crate) fn handle(&self) -> &metal::CommandQueueRef {
        &self.queue
    }
}

impl gpu::Queue<MTLBackend> for MTLQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, _flags: gpu::CommandPoolFlags) -> MTLCommandPool {
        MTLCommandPool::new(&self.queue, command_pool_type)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<MTLBackend>]) {
        for submission in submissions {
            if let Some(swapchain) = submission.acquire_swapchain {
                let mut present_state = swapchain.present_state().lock().unwrap();
                present_state.drawable = None;
                present_state.swapchain_release_scheduled = false;
                present_state.present_called = false;
            }

            if let Some(swapchain) = submission.release_swapchain {
                let c_present_state = swapchain.present_state().clone();

                if let Some(cmd_buffer) = submission.command_buffers.last() {
                    {
                        let mut present_state = c_present_state.lock().unwrap();
                        assert_eq!(present_state.present_called, false);
                        assert_eq!(present_state.swapchain_release_scheduled, false);
                        assert!(present_state.drawable.is_none());

                        let drawable = swapchain.take_drawable();
                        present_state.drawable = Some(drawable);
                        assert!(present_state.drawable.is_some());
                    }
                    let callback = move |_cmd_buffer: &metal::CommandBufferRef| {
                        let mut present_state = c_present_state.lock().unwrap();
                        assert_eq!(present_state.swapchain_release_scheduled, false);
                        if present_state.present_called {
                            assert!(present_state.drawable.is_some());
                            // Command Buffer was scheduled after the application called present()
                            present_state.drawable.take().unwrap().present();
                        } else {
                            // Command Buffer was scheduled before the application called present()
                            present_state.swapchain_release_scheduled = true;
                        }
                    };
                    let callback_block = ConcreteBlock::new(callback).copy();
                    cmd_buffer.handle().add_scheduled_handler(&callback_block);
                }
            }

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
        let mut present_state = swapchain.present_state().lock().unwrap();
        assert_eq!(present_state.present_called, false);
        if present_state.drawable.is_none() {
            // No submission used the swapchain with release_swapchain
            swapchain.take_drawable().present();
            return;
        }

        if present_state.swapchain_release_scheduled {
            // Command Buffer was scheduled before the application called present()
            present_state.drawable.take().unwrap().present();
        } else {
            // Command Buffer was scheduled after the application called present()
            present_state.present_called = true;
        }
    }
}
