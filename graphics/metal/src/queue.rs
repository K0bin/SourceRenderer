
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Condvar, Mutex};

use metal;
use block::ConcreteBlock;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Swapchain};

use super::*;

struct CompletionState {
    waiting_for_completion: Mutex<u64>,
    cond_var: Condvar
}

pub struct MTLQueue {
    queue: metal::CommandQueue,
    shared: Arc<MTLShared>,
    global_order_event: metal::Event,
    global_order_counter: AtomicU64,
    completion_state: Arc<CompletionState>
}

impl MTLQueue {
    pub(crate) fn new(device: &metal::DeviceRef, shared: &Arc<MTLShared>) -> Self {
        let queue = device.new_command_queue();
        Self {
            queue,
            shared: shared.clone(),
            global_order_event: device.new_event().to_owned(),
            global_order_counter: AtomicU64::new(0),
            completion_state: Arc::new(CompletionState {
                waiting_for_completion: Mutex::new(0u64),
                cond_var: Condvar::new()
            })
        }
    }

    pub fn wait_for_idle(&self) {
        let state = self.completion_state.waiting_for_completion.lock().unwrap();
        let _guard = self.completion_state.cond_var.wait_while(state, |waiting_for_completion|
            *waiting_for_completion != 0
        ).unwrap();
    }
}

impl gpu::Queue<MTLBackend> for MTLQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, _flags: gpu::CommandPoolFlags) -> MTLCommandPool {
        MTLCommandPool::new(&self.queue, command_pool_type, &self.shared)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<MTLBackend>]) {
        let mut waiting_for_completion = self.completion_state.waiting_for_completion.lock().unwrap();
        let counter_val = self.global_order_counter.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        for submission in submissions {
            for cmd_buf in submission.command_buffers {

                // We cannot add a wait for an event after encoding the command buffer, so each command buffer starts off with
                // a wait for its own event and we record a helper command buffer that does nothing but signal that event after waiting
                // for all events that are passed to the submission
                let fence_wait_cmd_buffer = self.queue.new_command_buffer();
                for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.wait_fences {
                    fence_wait_cmd_buffer.encode_wait_for_event(fence.event_handle(), *value);
                }
                fence_wait_cmd_buffer.set_label("Fence wait helper");

                // Because Metal doesn't have pipeline barriers and only guarantees that command buffers are started in order,
                // we synchronize between submissions. D3D12 does the same thing, so should hopefully be fine.
                // We do this to make sure events are signalled in order.
                fence_wait_cmd_buffer.encode_wait_for_event(&self.global_order_event, counter_val);
                fence_wait_cmd_buffer.encode_signal_event(cmd_buf.pre_event_handle(), 1);

                fence_wait_cmd_buffer.commit();

                // Fences are only supposed to be signalled after all command buffers in the submission are completed.
                // So if we have more than 1 command buffer, we need another helper command buffer that waits for each command buffer
                // and then does the actual signalling
                if submission.command_buffers.len() == 1 {
                    let mut fences: SmallVec<[(metal::SharedEvent, u64); 4]> = SmallVec::<[(metal::SharedEvent, u64); 4]>::with_capacity(submission.signal_fences.len());
                    for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.signal_fences {
                        if fence.is_shared() {
                            fences.push((fence.shared_handle().to_owned(), *value));
                        }
                    }
                    cmd_buf.handle().encode_signal_event(&self.global_order_event, counter_val + 1);

                    let callback = move |_cmd_buffer: &metal::CommandBufferRef| {
                        for (fence, value) in fences.iter() {
                            // We rely on shared events to make sure untracked resources are unused before destroying them.
                            // Apparently this doesn't work because shared events are signalled too early.
                            // So signal them on the CPU in the completion handler instead.
                            // This should be fine because the shared event isn't used for GPU<->GPU synchronization,
                            // where the latency would probably be terrible.
                            fence.set_signaled_value(*value);
                        }
                    };

                    let block = ConcreteBlock::new(callback).copy();
                    cmd_buf.handle().add_completed_handler(&block);
                } else {
                    cmd_buf.handle().encode_signal_event(cmd_buf.post_event_handle(), 1);
                }

                let c_state = self.completion_state.clone();
                let id = waiting_for_completion.trailing_ones();
                assert_eq!(*waiting_for_completion & (1 << (id as u64)), 0);

                *waiting_for_completion |= 1 << (id as u64);
                let block = ConcreteBlock::new(move |_cmd_buffer: &metal::CommandBufferRef| {
                    // Set the bit of the command buffer to 0 and notify the queue.
                    // The render thread might be waiting until the queue is idle.
                    {
                        let mut waiting_for_completion = c_state.waiting_for_completion.lock().unwrap();
                        assert_eq!((*waiting_for_completion >> (id as u64)) & 1, 1);
                        *waiting_for_completion &= !(1 << (id as u64));
                    }
                    c_state.cond_var.notify_all();
                }).copy();
                cmd_buf.handle().add_completed_handler(&block);
                cmd_buf.handle().commit();
            }
            if submission.command_buffers.len() > 1 {
                // We're submitting more than 1 command buffer and only want to signal the events after
                // all command buffers are done.
                // So similar to the wait helper, we use a helper command buffer that does nothing
                // except wait for all command buffers to signal that they're done and then signal
                // both the events that are specified in the submission and the global order event.
                let fence_signal_cmd_buffer = self.queue.new_command_buffer();
                for cmd_buf in submission.command_buffers {
                    fence_signal_cmd_buffer.encode_wait_for_event(cmd_buf.post_event_handle(), 1);
                }
                for gpu::FenceValuePairRef { fence, value, sync_before: _ } in submission.signal_fences {
                    fence_signal_cmd_buffer.encode_signal_event(fence.event_handle(), *value);
                }
                fence_signal_cmd_buffer.encode_signal_event(&self.global_order_event, counter_val + 1);
                fence_signal_cmd_buffer.set_label("Fence signal helper");
                fence_signal_cmd_buffer.commit();
            }
        }
    }

    unsafe fn present(&self, swapchain: &MTLSwapchain) {
        let drawable = swapchain.take_drawable();
        let backbuffer = swapchain.backbuffer(swapchain.backbuffer_index());

        let drawable_mtl_texture = drawable.texture();
        let dst = MTLTexture::from_mtl_texture(drawable_mtl_texture, false);
        let cmd_buffer = self.queue.new_command_buffer().to_owned();
        cmd_buffer.set_label("Present helper");
        // Begin/End are not actually necessary
        MTLCommandBuffer::blit_rp(&cmd_buffer, &self.shared, backbuffer, 0, 0, &dst, 0, 0);
        cmd_buffer.present_drawable(&drawable);
        cmd_buffer.commit();
    }
}
