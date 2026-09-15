use super::gpu::Queue as GPUQueue;
use super::*;
use crate::{Mutex, MutexGuard};
use smallvec::{SmallVec, smallvec};
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, TryLockError};

type SharedSwapchain = Arc<Mutex<super::Swapchain>>;
type Backbuffer = active_gpu_backend::Backbuffer;

pub type QueueFenceValue = (QueueType, u64);

pub struct FinishedCommandBuffer(ManuallyDrop<active_gpu_backend::CommandBuffer>);
impl FinishedCommandBuffer {
    pub(super) fn new(cmd_buffer: active_gpu_backend::CommandBuffer) -> Self {
        Self(ManuallyDrop::new(cmd_buffer))
    }
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

pub(super) struct Queue {
    inner: VecDeque<StoredQueueSubmission>,
    destroyer: Arc<DeferredDestroyer>,
    queue_type: QueueType,
}

pub(super) struct QueueTracker {
    fence: Arc<Fence>,
    next_counter: AtomicU64,
}

impl QueueTracker {
    pub(super) fn new(fence: Fence) -> Self {
        Self {
            fence: Arc::new(fence),
            next_counter: AtomicU64::new(1u64),
        }
    }
    pub(super) fn next_counter(&self) -> u64 {
        self.next_counter.load(Ordering::SeqCst)
    }
    pub(super) fn submitted_counter(&self) -> u64 {
        self.next_counter() - 1
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

    pub(super) fn wait_for_idle(&self) -> u64 {
        let value = self.next_counter.load(Ordering::SeqCst) - 1;
        self.fence.await_value(value);
        value
    }
}

impl Queue {
    pub(super) fn new(
        destroyer: &Arc<DeferredDestroyer>,
        queue_type: QueueType,
        fence: Fence,
    ) -> Self {
        Self {
            inner: VecDeque::new(),
            queue_type,
            destroyer: destroyer.clone(),
        }
    }

    pub(super) fn all_barrier_syncs(queue_type: QueueType) -> BarrierSync {
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

    pub(super) fn submit(&mut self, command_buffer: FinishedCommandBuffer) {
        let last = self.inner.iter_mut().last();
        if let Some(last) = last {
            if last.signal_fences.is_empty() && last.signal_swapchain.is_none() {
                last.command_buffers.push(command_buffer);
                return;
            }
        }

        self.inner.push_back(StoredQueueSubmission {
            command_buffers: smallvec![command_buffer],
            signal_fences: SmallVec::new(),
            wait_fences: SmallVec::new(),
            signal_swapchain: None,
            wait_swapchain: None,
        });
    }

    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub(super) fn submit_counter_bump(&mut self, tracker: &QueueTracker) {
        let value = tracker.next_counter.fetch_add(1u64, Ordering::SeqCst);

        let fence_value = SharedFenceValuePair {
            fence: tracker.fence.clone(),
            sync_before: Self::all_barrier_syncs(self.queue_type),
            value,
        };

        let last = self.inner.iter_mut().last();
        if let Some(last) = last {
            last.signal_fences.push(fence_value);
            return;
        }

        self.inner.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![fence_value],
            wait_fences: smallvec![],
            signal_swapchain: None,
            wait_swapchain: None,
        });
    }

    pub(super) fn wait_for(
        &mut self,
        fence: &Arc<super::Fence>,
        counter: u64,
        wait_before: BarrierSync,
    ) {
        let fence_value = SharedFenceValuePair {
            fence: fence.clone(),
            sync_before: wait_before & Self::all_barrier_syncs(self.queue_type),
            value: counter,
        };

        self.inner.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![],
            wait_fences: smallvec![fence_value],
            signal_swapchain: None,
            wait_swapchain: None,
        });
    }

    pub(super) fn acquire_swapchain(
        &mut self,
        swapchain: &SharedSwapchain,
        backbuffer: &Arc<Backbuffer>,
    ) {
        self.inner.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![],
            wait_fences: smallvec![],
            signal_swapchain: None,
            wait_swapchain: Some((swapchain.clone(), backbuffer.clone())),
        });
    }

    pub(super) fn release_swapchain(
        &mut self,
        swapchain: &SharedSwapchain,
        backbuffer: &Arc<Backbuffer>,
    ) {
        let last = self.inner.iter_mut().last();
        if let Some(last) = last {
            assert!(last.signal_swapchain.is_none());
            last.signal_swapchain = Some((swapchain.clone(), backbuffer.clone()));
            return;
        }

        self.inner.push_back(StoredQueueSubmission {
            command_buffers: smallvec![],
            signal_fences: smallvec![],
            wait_fences: smallvec![],
            signal_swapchain: Some((swapchain.clone(), backbuffer.clone())),
            wait_swapchain: None,
        });
    }

    pub(super) fn present(
        &mut self,
        swapchain: &Arc<Mutex<super::Swapchain>>,
        backbuffer: Arc<active_gpu_backend::Backbuffer>,
        queue: &active_gpu_backend::Queue,
    ) {
        //Self::flush_locked(&mut guard, self.queue_type, queue);

        let mut swapchain_inner = swapchain.lock().unwrap();
        unsafe {
            queue.present(swapchain_inner.handle_mut(), backbuffer.as_ref());
        }
    }

    pub(super) fn flush(
        &mut self,
        queue: &active_gpu_backend::Queue,
        tracker: &QueueTracker,
    ) -> u64 {
        if self.inner.is_empty() {
            return tracker.submitted_counter();
        }

        let mut cmd_buffer_refs =
            SmallVec::<[&active_gpu_backend::CommandBuffer; 2]>::with_capacity(self.inner.len());
        let mut cmd_buffer_ranges = SmallVec::<[Range<usize>; 2]>::with_capacity(self.inner.len());

        let mut swapchain_guards =
            SmallVec::<[MutexGuard<super::Swapchain>; 2]>::with_capacity(self.inner.len());
        let mut swapchain_guard_indices = SmallVec::<
            [Option<(usize, &active_gpu_backend::Backbuffer)>; 2],
        >::with_capacity(self.inner.len() * 2);
        let mut fence_refs = SmallVec::<[active_gpu_backend::FenceValuePairRef; 2]>::with_capacity(
            self.inner.len() * 2,
        );
        let mut fence_ranges = SmallVec::<[Range<usize>; 2]>::with_capacity(self.inner.len() * 2);
        for submission in self.inner.iter() {
            let mut cmd_buffer_start = cmd_buffer_refs.len();
            for cmd_buffer in &submission.command_buffers {
                cmd_buffer_refs.push(&*cmd_buffer.0);
            }
            cmd_buffer_ranges.push(cmd_buffer_start..cmd_buffer_refs.len());

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

        let mut gpu_submissions =
            SmallVec::<[active_gpu_backend::Submission; 2]>::with_capacity(self.inner.len());
        for submission in self.inner.iter() {
            if let Some((swapchain, backbuffer)) = submission.wait_swapchain.as_ref() {
                let index = match swapchain.try_lock() {
                    Ok(swapchain_lock) => {
                        swapchain_guards.push(swapchain_lock);
                        swapchain_guards.len() - 1
                    }
                    Err(e) => {
                        match e {
                            TryLockError::WouldBlock => {}
                            _ => panic!("{:?}", e),
                        }
                        assert!(!swapchain_guards.is_empty());
                        // TODO: Find the swapchain.
                        0
                    }
                };
                swapchain_guard_indices.push(Some((index, &backbuffer)));
            } else {
                swapchain_guard_indices.push(None);
            }

            if let Some((swapchain, backbuffer)) = submission.signal_swapchain.as_ref() {
                let index = match swapchain.try_lock() {
                    Ok(swapchain_lock) => {
                        swapchain_guards.push(swapchain_lock);
                        swapchain_guards.len() - 1
                    }
                    Err(e) => {
                        match e {
                            TryLockError::WouldBlock => {}
                            _ => panic!("{:?}", e),
                        }
                        assert!(!swapchain_guards.is_empty());
                        // TODO: Find the swapchain.
                        0
                    }
                };
                swapchain_guard_indices.push(Some((index, &backbuffer)));
            } else {
                swapchain_guard_indices.push(None);
            }
        }
        for (idx, _) in self.inner.iter().enumerate() {
            let acquire_swapchain =
                swapchain_guard_indices[idx * 2]
                    .as_ref()
                    .map(|(swapchain_index, backbuffer)| {
                        (swapchain_guards[*swapchain_index].handle(), *backbuffer)
                    });
            let release_swapchain = swapchain_guard_indices[idx * 2 + 1].as_ref().map(
                |(swapchain_index, backbuffer)| {
                    (swapchain_guards[*swapchain_index].handle(), *backbuffer)
                },
            );

            gpu_submissions.push(active_gpu_backend::Submission {
                command_buffers: &cmd_buffer_refs[cmd_buffer_ranges[idx].clone()],
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
        std::mem::drop(swapchain_guard_indices);
        std::mem::drop(swapchain_guards);
        std::mem::drop(cmd_buffer_refs);
        std::mem::drop(cmd_buffer_ranges);

        Self::destroy_cmd_buffers(&self.destroyer, self.inner.drain(..));

        self.inner.clear();
        tracker.submitted_counter()
    }

    fn destroy_cmd_buffers(
        destroyer: &DeferredDestroyer,
        submissions: impl Iterator<Item = StoredQueueSubmission>,
    ) {
        for mut submission in submissions {
            for mut cmd_buffer in submission.command_buffers.drain(..) {
                let cmd_buffer_handle = unsafe { ManuallyDrop::take(&mut cmd_buffer.0) };
                destroyer.destroy_command_buffer(cmd_buffer_handle);
            }
        }
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn queue_type(&self) -> QueueType {
        self.queue_type
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        Self::destroy_cmd_buffers(&self.destroyer, self.inner.drain(..));
    }
}
