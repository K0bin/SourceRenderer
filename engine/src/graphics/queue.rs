use super::gpu::Queue as GPUQueue;
use super::*;
use crate::{Condvar, Mutex, MutexGuard};
use atomic_refcell::AtomicRefCell;
use smallvec::{SmallVec, smallvec};
use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

type SharedSwapchain = Arc<Mutex<super::Swapchain>>;
type SharedSwapchainPtr = *const Mutex<super::Swapchain>;
type GPUSwapchainPtr = *const active_gpu_backend::Swapchain;
type Backbuffer = active_gpu_backend::Backbuffer;

struct CountedCommandPool {
    counter: CommandPoolCounter,
    pool: Arc<AtomicRefCell<active_gpu_backend::CommandPool>>,
}

impl Drop for CountedCommandPool {
    fn drop(&mut self) {
        assert_eq!(self.counter.value(), 0);
    }
}

#[derive(Clone, Default)]
pub struct CommandPoolCounter(Arc<AtomicU64>);
impl CommandPoolCounter {
    pub fn value(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    pub(super) fn increment(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}
impl Drop for CommandPoolCounter {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct FinishedCommandBuffer {
    pub(super) handle: active_gpu_backend::CommandBuffer,
    pub(super) pool: Arc<active_gpu_backend::CommandPool>,
    pub(super) command_pool_counter: CommandPoolCounter,
}

struct StoredQueueSubmission {
    command_buffers: SmallVec<[FinishedCommandBuffer; 4]>,
    signal_swapchain: Option<(SharedSwapchain, Arc<Backbuffer>)>,
    wait_swapchain: Option<(SharedSwapchain, Arc<Backbuffer>)>,
    signal_fences: SmallVec<[SharedFenceValuePair; 4]>,
    wait_fences: SmallVec<[SharedFenceValuePair; 4]>,
}

pub struct QueueSubmission<'a> {
    pub command_buffer: FinishedCommandBuffer,
    pub wait_fences: &'a [SharedFenceValuePairRef<'a>],
    pub acquire_swapchain: Option<(&'a SharedSwapchain, &'a Arc<Backbuffer>)>,
    pub release_swapchain: Option<(&'a SharedSwapchain, &'a Arc<Backbuffer>)>,
}

struct QueueInner {
    virtual_queue: VecDeque<StoredQueueSubmission>,
    is_idle: bool,
}

pub(super) struct Queue {
    inner: Mutex<QueueInner>,
    queue_type: QueueType,
    idle_condvar: Condvar,
    fence: Arc<Fence>,
    next_counter: AtomicU64,
}

impl Queue {
    pub(super) fn new(queue_type: QueueType, fence: Fence) -> Self {
        Self {
            inner: Mutex::new(QueueInner {
                virtual_queue: VecDeque::new(),
                is_idle: true,
            }),
            queue_type,
            idle_condvar: Condvar::new(),
            fence: Arc::new(fence),
            next_counter: AtomicU64::new(1u64),
        }
    }

    fn all_barrier_syncs(queue_type: QueueType) -> BarrierSync {
        match queue_type {
            QueueType::Graphics => BarrierSync::all(),
            QueueType::Compute => {
                BarrierSync::COMPUTE_SHADER
                    | BarrierSync::ACCELERATION_STRUCTURE_BUILD
                    | BarrierSync::RAY_TRACING_SHADER
                    | BarrierSync::COPY
            }
            QueueType::Transfer => BarrierSync::COPY,
        }
    }

    pub(super) fn submit(&self, command_buffer: FinishedCommandBuffer) {
        let mut guard = self.inner.lock().unwrap();
        guard.is_idle = false;

        let last = guard.virtual_queue.iter_mut().last();
        if let Some(last) = last {
            if last.signal_fences.is_empty() && last.signal_swapchain.is_none() {
                last.command_buffers.push(command_buffer);
                return;
            }
        }

        guard.virtual_queue.push_back(StoredQueueSubmission {
            command_buffers: smallvec![command_buffer],
            signal_fences: SmallVec::new(),
            wait_fences: SmallVec::new(),
            signal_swapchain: None,
            wait_swapchain: None,
        });
    }

    pub(super) fn submit_counter_bump(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.is_idle = false;

        let value = self.next_counter.fetch_add(1u64, Ordering::SeqCst);
        let fence_value = SharedFenceValuePair {
            fence: self.fence.clone(),
            sync_before: Self::all_barrier_syncs(self.queue_type),
            value,
        };

        let last = guard.virtual_queue.iter_mut().last();
        if let Some(last) = last {
            for fence in last.signal_fences.iter() {
                if fence == &fence_value {
                    return;
                }
            }

            last.signal_fences.push(fence_value);
        }

        guard.virtual_queue.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![SharedFenceValuePair {
                fence: self.fence.clone(),
                sync_before: Self::all_barrier_syncs(self.queue_type),
                value
            }],
            wait_fences: smallvec![],
            signal_swapchain: None,
            wait_swapchain: None,
        });
    }

    pub(super) fn acquire_swapchain(
        &self,
        swapchain: &SharedSwapchain,
        backbuffer: &Arc<Backbuffer>,
    ) {
        let mut guard = self.inner.lock().unwrap();
        guard.is_idle = false;

        guard.virtual_queue.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![],
            wait_fences: smallvec![],
            signal_swapchain: None,
            wait_swapchain: Some((swapchain.clone(), backbuffer.clone())),
        });
    }

    pub(super) fn release_swapchain(
        &self,
        swapchain: &SharedSwapchain,
        backbuffer: &Arc<Backbuffer>,
    ) {
        let mut guard = self.inner.lock().unwrap();
        guard.is_idle = false;

        let last = guard.virtual_queue.iter_mut().last();
        if let Some(last) = last {
            assert!(last.signal_swapchain.is_none());
            last.signal_swapchain = Some((swapchain.clone(), backbuffer.clone()));
            return;
        }

        guard.virtual_queue.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![],
            wait_fences: smallvec![],
            signal_swapchain: Some((swapchain.clone(), backbuffer.clone())),
            wait_swapchain: None,
        });
    }

    pub(super) fn next_counter(&self) -> u64 {
        self.next_counter.load(Ordering::SeqCst)
    }

    pub(super) fn completed_counter(&self) -> u64 {
        self.fence.value()
    }

    pub(super) fn await_counter(&self, value: u64) {
        self.fence.await_value(value)
    }

    pub(super) fn fence(&self) -> &Arc<Fence> {
        &self.fence
    }

    pub(super) fn present(
        &self,
        swapchain: &Arc<Mutex<super::Swapchain>>,
        backbuffer: Arc<active_gpu_backend::Backbuffer>,
        queue: &active_gpu_backend::Queue,
    ) {
        self.flush(queue);

        let mut swapchain_inner = swapchain.lock().unwrap();
        unsafe {
            queue.present(swapchain_inner.handle_mut(), backbuffer.as_ref());
        }
    }

    pub(super) fn flush(&self, queue: &active_gpu_backend::Queue) {
        let mut guard = self.inner.lock().unwrap();
        if guard.virtual_queue.is_empty() {
            return;
        }

        let mut cmd_buffers: SmallVec<[active_gpu_backend::CommandBuffer; 4]> =
            SmallVec::with_capacity(guard.virtual_queue.len());
        let mut cmd_buffers_ranges: SmallVec<[Range<usize>; 4]> =
            SmallVec::with_capacity(guard.virtual_queue.len());
        for submission in guard.virtual_queue.iter_mut() {
            let range_start = cmd_buffers.len();
            for cmd_buffer in submission.command_buffers.drain(..) {
                cmd_buffers.push(cmd_buffer.handle);
            }
            cmd_buffers_ranges.push(range_start..cmd_buffers.len());
        }

        let mut swapchain_guards = SmallVec::<
            [Option<(
                MutexGuard<super::Swapchain>,
                &active_gpu_backend::Backbuffer,
            )>; 2],
        >::with_capacity(guard.virtual_queue.len() * 2);
        let mut fence_refs = SmallVec::<[active_gpu_backend::FenceValuePairRef; 2]>::with_capacity(
            guard.virtual_queue.len() * 2,
        );
        let mut fence_ranges =
            SmallVec::<[Range<usize>; 2]>::with_capacity(guard.virtual_queue.len() * 2);
        for submission in guard.virtual_queue.iter() {
            let mut start = fence_refs.len();
            for fence in &submission.wait_fences {
                fence_refs.push(active_gpu_backend::FenceValuePairRef {
                    fence: fence.fence.handle(),
                    value: fence.value,
                    sync_before: fence.sync_before,
                });
            }
            fence_ranges.push(start..fence_refs.len());

            start = fence_refs.len();
            for fence in &submission.signal_fences {
                fence_refs.push(active_gpu_backend::FenceValuePairRef {
                    fence: fence.fence.handle(),
                    value: fence.value,
                    sync_before: fence.sync_before,
                });
            }
            fence_ranges.push(start..fence_refs.len());
        }

        let mut gpu_submissions = SmallVec::<[active_gpu_backend::Submission; 2]>::with_capacity(
            guard.virtual_queue.len(),
        );
        for submission in guard.virtual_queue.iter() {
            if let Some((swapchain, backbuffer)) = submission.signal_swapchain.as_ref() {
                swapchain_guards.push(Some((swapchain.lock().unwrap(), &backbuffer)));
            } else {
                swapchain_guards.push(None);
            }

            if let Some((swapchain, backbuffer)) = submission.wait_swapchain.as_ref() {
                swapchain_guards.push(Some((swapchain.lock().unwrap(), &backbuffer)));
            } else {
                swapchain_guards.push(None);
            }
        }
        for (idx, _) in guard.virtual_queue.iter().enumerate() {
            let acquire_swapchain = swapchain_guards[idx * 2]
                .as_ref()
                .map(|(swapchain, backbuffer)| (swapchain.handle(), *backbuffer));
            let release_swapchain = swapchain_guards[idx * 2]
                .as_ref()
                .map(|(swapchain, backbuffer)| (swapchain.handle(), *backbuffer));

            gpu_submissions.push(active_gpu_backend::Submission {
                command_buffers: &cmd_buffers[cmd_buffers_ranges[idx].clone()],
                wait_fences: &fence_refs[fence_ranges[idx * 2].clone()],
                signal_fences: &fence_refs[fence_ranges[idx * 2 + 1].clone()],
                acquire_swapchain,
                release_swapchain,
            });
        }

        unsafe {
            queue.submit(&gpu_submissions);
        }
        std::mem::drop(gpu_submissions);
        std::mem::drop(fence_refs);
        std::mem::drop(swapchain_guards);
        std::mem::drop(cmd_buffers);
        std::mem::drop(cmd_buffers_ranges);
        guard.virtual_queue.clear();
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn queue_type(&self) -> QueueType {
        self.queue_type
    }

    pub(super) fn wait_for_idle(&self) -> u64 {
        let guard = self.inner.lock().unwrap();
        let _new_guard = self.idle_condvar.wait_while(guard, |g| !g.is_idle).unwrap();
        self.fence
            .await_value(self.next_counter.load(Ordering::SeqCst) - 1);
        self.fence.value()
    }
}
