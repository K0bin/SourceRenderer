use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use sourcerenderer_core::graphics::Resettable;
use thread_local::ThreadLocal;

use crate::buffer::BufferAllocator;
use crate::command::VkInnerCommandBufferInfo;
use crate::query::VkQueryAllocator;
use crate::queue::VkQueueInfo;
use crate::raw::RawVkDevice;
use crate::sync::VkTimelineSemaphore;
use crate::{
    VkCommandBufferRecorder,
    VkCommandPool,
    VkLifetimeTrackers,
    VkShared,
};

pub struct VkThreadManager {
    device: Arc<RawVkDevice>,
    graphics_queue: VkQueueInfo,
    compute_queue: Option<VkQueueInfo>,
    transfer_queue: Option<VkQueueInfo>,
    threads: ThreadLocal<VkThreadLocal>,
    shared: Arc<VkShared>,
    max_prepared_frames: u32,
    frame_counter: AtomicU64,
    timeline_semaphore: Arc<VkTimelineSemaphore>,
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadLocal {
    device: Arc<RawVkDevice>,
    frame_counter: RefCell<u64>,
    frames: Vec<VkFrameLocal>,
    disable_sync: PhantomData<*const u32>,
}
unsafe impl Send for VkThreadLocal {}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameLocal {
    device: Arc<RawVkDevice>,
    buffer_allocator: Arc<BufferAllocator>,
    inner: RefCell<VkFrameLocalInner>,
    disable_sync: PhantomData<*const u32>,
    query_allocator: Arc<VkQueryAllocator>,
}
unsafe impl Send for VkFrameLocal {}

struct VkFrameLocalInner {
    command_pool: VkCommandPool,
    life_time_trackers: VkLifetimeTrackers,
    frame: u64,
}

impl VkThreadManager {
    pub fn new(
        device: &Arc<RawVkDevice>,
        graphics_queue: &VkQueueInfo,
        compute_queue: Option<&VkQueueInfo>,
        transfer_queue: Option<&VkQueueInfo>,
        shared: &Arc<VkShared>,
        max_prepared_frames: u32,
    ) -> Self {
        VkThreadManager {
            device: device.clone(),
            threads: ThreadLocal::new(),
            graphics_queue: *graphics_queue,
            compute_queue: compute_queue.cloned(),
            transfer_queue: transfer_queue.cloned(),
            shared: shared.clone(),
            max_prepared_frames,
            frame_counter: AtomicU64::new(1),
            timeline_semaphore: Arc::new(VkTimelineSemaphore::new(device)),
        }
    }

    pub fn begin_frame(&self) {
        let new_frame = self.frame_counter.load(Ordering::SeqCst);

        if new_frame > self.max_prepared_frames as u64 {
            self.timeline_semaphore
                .await_value(new_frame - self.max_prepared_frames as u64);
        }
    }

    pub fn get_thread_local(&self) -> &VkThreadLocal {
        let thread_local = self.threads.get_or(|| {
            VkThreadLocal::new(
                &self.device,
                &self.shared,
                &self.graphics_queue,
                self.compute_queue.as_ref(),
                self.transfer_queue.as_ref(),
                self.max_prepared_frames,
            )
        });
        thread_local.set_frame(self.frame_counter.load(Ordering::SeqCst));
        thread_local
    }

    pub fn end_frame(&self) -> u64 {
        self.frame_counter.fetch_add(1, Ordering::SeqCst)
    }

    #[inline]
    pub fn shared(&self) -> &Arc<VkShared> {
        &self.shared
    }

    pub fn prerendered_frames(&self) -> u32 {
        self.max_prepared_frames
    }

    pub fn frame(&self) -> u64 {
        self.frame_counter.load(Ordering::SeqCst)
    }

    pub fn timeline_semaphore(&self) -> &Arc<VkTimelineSemaphore> {
        &self.timeline_semaphore
    }
}

impl VkThreadLocal {
    fn new(
        device: &Arc<RawVkDevice>,
        shared: &Arc<VkShared>,
        graphics_queue: &VkQueueInfo,
        compute_queue: Option<&VkQueueInfo>,
        transfer_queue: Option<&VkQueueInfo>,
        max_prepared_frames: u32,
    ) -> Self {
        let mut frames: Vec<VkFrameLocal> = Vec::new();
        for _ in 0..max_prepared_frames {
            frames.push(VkFrameLocal::new(
                device,
                shared,
                graphics_queue,
                compute_queue,
                transfer_queue,
            ))
        }

        VkThreadLocal {
            device: device.clone(),
            frames,
            frame_counter: RefCell::new(0u64),
            disable_sync: PhantomData,
        }
    }

    fn set_frame(&self, frame: u64) {
        let mut frame_counter = self.frame_counter.borrow_mut();
        debug_assert!(frame >= *frame_counter);
        let length = self.frames.len();
        if frame > *frame_counter && frame >= self.frames.len() as u64 {
            let frame_ref = &self.frames[frame as usize % length];
            frame_ref.reset();
        }
        *frame_counter = frame;
    }

    pub fn get_frame_local(&self) -> &VkFrameLocal {
        let frame_counter = self.frame_counter.borrow();
        let length = self.frames.len();
        let frame_local = &self.frames[*frame_counter as usize % length];
        frame_local.set_frame(*frame_counter);
        frame_local
    }
}

impl VkFrameLocal {
    pub fn new(
        device: &Arc<RawVkDevice>,
        shared: &Arc<VkShared>,
        graphics_queue: &VkQueueInfo,
        _compute_queue: Option<&VkQueueInfo>,
        _transfer_queue: Option<&VkQueueInfo>,
    ) -> Self {
        let buffer_allocator = Arc::new(BufferAllocator::new(device, false));
        let query_allocator = Arc::new(VkQueryAllocator::new(device));
        let command_pool = VkCommandPool::new(
            device,
            graphics_queue.queue_family_index as u32,
            shared,
            &buffer_allocator,
            &query_allocator,
        );
        Self {
            device: device.clone(),
            buffer_allocator,
            inner: RefCell::new(VkFrameLocalInner {
                command_pool,
                life_time_trackers: VkLifetimeTrackers::new(),
                frame: 0,
            }),
            query_allocator,
            disable_sync: PhantomData,
        }
    }

    fn set_frame(&self, frame: u64) {
        let mut inner = self.inner.borrow_mut();
        debug_assert!(frame >= inner.frame);
        inner.frame = frame;
    }

    pub fn get_command_buffer(&self) -> VkCommandBufferRecorder {
        let mut inner = self.inner.borrow_mut();
        let frame = inner.frame;
        inner.command_pool.get_command_buffer(frame)
    }

    pub fn get_inner_command_buffer(
        &self,
        inner_info: Option<&VkInnerCommandBufferInfo>,
    ) -> VkCommandBufferRecorder {
        let mut inner = self.inner.borrow_mut();
        let frame = inner.frame;
        inner
            .command_pool
            .get_inner_command_buffer(frame, inner_info)
    }

    pub fn track_semaphore(&self, semaphore: &Arc<VkTimelineSemaphore>) {
        let mut inner = self.inner.borrow_mut();
        inner.life_time_trackers.track_semaphore(semaphore);
    }

    pub fn reset(&self) {
        self.buffer_allocator.reset();
        self.query_allocator.reset();
        let mut inner = self.inner.borrow_mut();
        inner.life_time_trackers.reset();
        inner.command_pool.reset();
    }
}

impl Drop for VkFrameLocal {
    fn drop(&mut self) {
        self.device.wait_for_idle();
    }
}
