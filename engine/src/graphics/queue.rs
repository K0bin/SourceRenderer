use std::{collections::VecDeque, ops::Range, sync::Arc};
use crate::{Mutex, MutexGuard, Condvar};

use crossbeam_channel::Sender;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Queue as GPUQueue};

use super::*;

type SharedSwapchain = Arc<Mutex<super::Swapchain>>;
type SharedSwapchainPtr = *const Mutex<super::Swapchain>;
type GPUSwapchainPtr = *const active_gpu_backend::Swapchain;
type Backbuffer = active_gpu_backend::Backbuffer;

enum StoredQueueSubmission {
    CommandBuffer {
        command_buffer: Box<super::CommandBuffer>,
        return_sender: Sender<Box<super::CommandBuffer>>,
        signal_swapchain: Option<(Arc<Mutex<super::Swapchain>>, Arc<Backbuffer>)>,
        wait_swapchain: Option<(Arc<Mutex<super::Swapchain>>, Arc<Backbuffer>)>,
        signal_fences: SmallVec<[SharedFenceValuePair; 4]>,
        wait_fences: SmallVec<[SharedFenceValuePair; 4]>,
    },
    Present {
        swapchain: (Arc<Mutex<super::Swapchain>>, Arc<active_gpu_backend::Backbuffer>),
    }
}

pub struct QueueSubmission<'a> {
    pub command_buffer: FinishedCommandBuffer,
    pub wait_fences: &'a [SharedFenceValuePairRef<'a>],
    pub signal_fences: &'a [SharedFenceValuePairRef<'a>],
    pub acquire_swapchain: Option<(&'a SharedSwapchain, &'a Arc<Backbuffer>)>,
    pub release_swapchain: Option<(&'a SharedSwapchain, &'a Arc<Backbuffer>)>,
}

struct QueueInner {
    virtual_queue: VecDeque<StoredQueueSubmission>,
    is_idle: bool
}

pub(super) struct Queue {
    inner: Mutex<QueueInner>,
    queue_type: QueueType,
    idle_condvar: Condvar
}

impl Queue {
    pub(super) fn new(queue_type: QueueType) -> Self {
        Self {
            inner: Mutex::new(QueueInner {
                virtual_queue: VecDeque::new(),
                is_idle: true
            }),
            queue_type,
            idle_condvar: Condvar::new()
        }
    }

    pub(super) fn submit(&self, submission: QueueSubmission) {
        let mut guard = self.inner.lock().unwrap();
        guard.is_idle = false;

        let QueueSubmission { command_buffer: finished_cmd_buffer, wait_fences, signal_fences, acquire_swapchain, release_swapchain } = submission;
        let FinishedCommandBuffer { inner, sender } = finished_cmd_buffer;

        guard.virtual_queue.push_back(StoredQueueSubmission::CommandBuffer {
            command_buffer: inner,
            return_sender: sender,
            signal_fences: signal_fences.iter().map(|fence_ref| SharedFenceValuePair::from(fence_ref)).collect(),
            wait_fences: wait_fences.iter().map(|fence_ref| SharedFenceValuePair::from(fence_ref)).collect(),
            signal_swapchain: release_swapchain.map(|(swapchain, key)| (swapchain.clone(), key.clone())),
            wait_swapchain: acquire_swapchain.map(|(swapchain, key)| (swapchain.clone(), key.clone())),
        });
    }

    pub(super) fn present(&self, swapchain: &Arc<Mutex<super::Swapchain>>, backbuffer: Arc<active_gpu_backend::Backbuffer>) {
        let mut guard: crate::MutexGuard<'_, QueueInner> = self.inner.lock().unwrap();
        guard.is_idle = false;
        guard.virtual_queue.push_back(StoredQueueSubmission::Present { swapchain: (swapchain.clone(), backbuffer) });
    }

    pub(super) fn flush(&self, queue: &active_gpu_backend::Queue) {
        let mut guard = self.inner.lock().unwrap();

        const COMMAND_BUFFER_CAPACITY: usize = 16;
        const SUBMISSION_CAPACITY: usize = 16;
        const FENCE_CAPACITY: usize = 16;

        type SwapchainGuard<'a> = MutexGuard<'a, Swapchain>;

        struct SubmissionHolder<'a> {
            queue: &'a active_gpu_backend::Queue,
            command_buffers: SmallVec<[&'a active_gpu_backend::CommandBuffer; COMMAND_BUFFER_CAPACITY]>,
            cmd_buffer_range: Range<usize>,
            submissions: SmallVec<[active_gpu_backend::Submission<'a>; SUBMISSION_CAPACITY]>,
            fences: SmallVec<[active_gpu_backend::FenceValuePairRef<'a>; FENCE_CAPACITY]>,
            swapchain_guards: SmallVec::<[(SharedSwapchainPtr, SwapchainGuard<'a>); SUBMISSION_CAPACITY]>
        }

        fn flush_command_buffers<'a>(holder: &mut SubmissionHolder<'a>) {
            if holder.command_buffers.is_empty() || holder.cmd_buffer_range.len() == 0 {
                return;
            }

            if holder.submissions.len() == SUBMISSION_CAPACITY {
                flush_submissions(holder);
            }

            holder.submissions.push(gpu::Submission::<'a> {
                command_buffers: unsafe { std::slice::from_raw_parts(holder.command_buffers.as_ptr().add(holder.cmd_buffer_range.start), holder.cmd_buffer_range.end - holder.cmd_buffer_range.start) },
                wait_fences: &[],
                signal_fences: &[],
                acquire_swapchain: None,
                release_swapchain: None
            });

            holder.cmd_buffer_range.start = holder.cmd_buffer_range.end;
        }

        fn flush_submissions<'a>(holder: &mut SubmissionHolder<'a>) {
            if holder.submissions.is_empty() {
                return;
            }
            unsafe {
                holder.queue.submit(&holder.submissions[..]);
            }
            holder.submissions.clear();
            holder.command_buffers.clear();
            holder.cmd_buffer_range.start = 0;
            holder.cmd_buffer_range.end = 0;
            holder.swapchain_guards.clear();
        }

        fn push_command_buffer<'a>(holder: &mut SubmissionHolder<'a>, command_buffer: &'a active_gpu_backend::CommandBuffer) {
            if holder.command_buffers.len() == COMMAND_BUFFER_CAPACITY {
                flush_command_buffers(holder);
            }
            holder.command_buffers.push(command_buffer);
            holder.cmd_buffer_range.end += 1;
        }

        fn push_submission<'a>(
            holder: &mut SubmissionHolder<'a>,
            command_buffer: &'a active_gpu_backend::CommandBuffer,
            wait_fences: &'a [SharedFenceValuePair],
            signal_fences: &'a [SharedFenceValuePair],
            acquire_swapchain: Option<(&'a SharedSwapchain, &'a Backbuffer)>,
            signal_swapchain: Option<(&'a SharedSwapchain, &'a Backbuffer)>,
        ) {
            if !holder.command_buffers.is_empty() {
                flush_command_buffers(holder);
            }
            if holder.submissions.len() == SUBMISSION_CAPACITY || FENCE_CAPACITY - holder.fences.len() < wait_fences.len() + signal_fences.len() || holder.command_buffers.len() == COMMAND_BUFFER_CAPACITY {
                flush_submissions(holder);
            }
            let wait_fences_start = holder.fences.len();
            for fence in wait_fences {
                holder.fences.push(sourcerenderer_core::gpu::FenceValuePairRef::<'static> {
                    fence: unsafe { std::mem::transmute(fence.fence.handle()) },
                    value: fence.value,
                    sync_before: fence.sync_before
                });
            }
            let signal_fences_start = holder.fences.len();
            for fence in signal_fences {
                holder.fences.push(sourcerenderer_core::gpu::FenceValuePairRef::<'a> {
                    fence: fence.fence.handle(),
                    value: fence.value,
                    sync_before: fence.sync_before
                });
            }

            let mut acquire_swapchain_handle_ptr: Option<(GPUSwapchainPtr, &'a Backbuffer)> = None;
            if let Some((swapchain, backbuffer)) = acquire_swapchain {
                let ptr: SharedSwapchainPtr = Arc::as_ptr(swapchain);
                for (existing_ptr, existing_guard) in &holder.swapchain_guards {
                    if *existing_ptr == ptr {
                        acquire_swapchain_handle_ptr = Some((existing_guard.handle() as GPUSwapchainPtr, backbuffer));
                        break;
                    }
                }
                if acquire_swapchain_handle_ptr.is_none() {
                    let guard = swapchain.lock().unwrap();
                    acquire_swapchain_handle_ptr = Some((guard.handle() as GPUSwapchainPtr, backbuffer));
                    holder.swapchain_guards.push((ptr, guard));
                }
            }

            let mut signal_swapchain_handle_ptr: Option<(GPUSwapchainPtr, &'a Backbuffer)> = None;
            if let Some((swapchain, backbuffer)) = signal_swapchain {
                let ptr: SharedSwapchainPtr = Arc::as_ptr(swapchain);
                for (existing_ptr, existing_guard) in &holder.swapchain_guards {
                    if *existing_ptr == ptr {
                        signal_swapchain_handle_ptr = Some((existing_guard.handle() as GPUSwapchainPtr, backbuffer));
                        break;
                    }
                }
                if signal_swapchain_handle_ptr.is_none() {
                    let guard = swapchain.lock().unwrap();
                    signal_swapchain_handle_ptr = Some((guard.handle() as GPUSwapchainPtr, backbuffer));
                    holder.swapchain_guards.push((ptr, guard));
                }
            }

            holder.command_buffers.push(command_buffer);
            holder.submissions.push(gpu::Submission::<'a> {
                command_buffers: unsafe { std::slice::from_raw_parts(holder.command_buffers.as_ptr().add(holder.command_buffers.len() - 1), 1) },
                wait_fences: unsafe { std::slice::from_raw_parts(holder.fences.as_ptr().add(wait_fences_start), wait_fences.len()) },
                signal_fences: unsafe { std::slice::from_raw_parts(holder.fences.as_ptr().add(signal_fences_start), signal_fences.len()) },
                acquire_swapchain: acquire_swapchain_handle_ptr.map(|(ptr, backbuffer)| (unsafe { ptr.as_ref().unwrap() }, backbuffer)),
                release_swapchain: signal_swapchain_handle_ptr.map(|(ptr, backbuffer)| (unsafe { ptr.as_ref().unwrap() }, backbuffer)),
            });
        }

        let mut holder = SubmissionHolder {
            queue,
            command_buffers: SmallVec::new(),
            cmd_buffer_range: 0..0,
            submissions: SmallVec::new(),
            fences: SmallVec::new(),
            swapchain_guards: SmallVec::new(),
        };

        for submission in guard.virtual_queue.iter() {
            match submission {
                StoredQueueSubmission::CommandBuffer {
                    command_buffer,
                    return_sender: _,
                    signal_fences,
                    signal_swapchain,
                    wait_fences,
                    wait_swapchain
                } => {
                    if wait_fences.is_empty() && signal_fences.is_empty() && wait_swapchain.is_none() && signal_swapchain.is_none() {
                        push_command_buffer(&mut holder, command_buffer.handle());
                    } else {
                        push_submission(&mut holder, command_buffer.handle(), wait_fences, signal_fences, wait_swapchain.as_ref().map(|(s, key)| (s, key.as_ref())), signal_swapchain.as_ref().map(|(s, key)| (s, key.as_ref())));
                    }
                },
                StoredQueueSubmission::Present { swapchain: (swapchain, key) } => {
                    if !holder.command_buffers.is_empty() {
                        flush_command_buffers(&mut holder);
                    }
                    if !holder.submissions.is_empty() {
                        flush_submissions(&mut holder);
                    }

                    let mut swapchain_guard = swapchain.lock().unwrap();
                    unsafe {
                        queue.present(swapchain_guard.handle_mut(), &key);
                    }
                }
            }
        }

        if !holder.cmd_buffer_range.is_empty() {
            flush_command_buffers(&mut holder);
        }

        if !holder.submissions.is_empty() {
            flush_submissions(&mut holder);
        }
        std::mem::drop(holder);

        for submission in guard.virtual_queue.drain(..) {
            match submission {
                StoredQueueSubmission::CommandBuffer { command_buffer, return_sender, .. } => {
                    return_sender.send(command_buffer).unwrap();
                },
                StoredQueueSubmission::Present { swapchain: _ } => {}
            }
        }

        guard.is_idle = guard.virtual_queue.is_empty();
        if guard.is_idle {
            self.idle_condvar.notify_all();
        }
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn queue_type(&self) -> QueueType {
        self.queue_type
    }

    pub(super) fn wait_for_idle(&self) {
        let guard = self.inner.lock().unwrap();
        let _new_guard = self.idle_condvar.wait_while(guard, |g| !g.is_idle).unwrap();
    }
}
