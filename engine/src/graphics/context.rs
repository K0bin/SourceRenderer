use std::collections::VecDeque;
#[cfg(target_arch = "wasm32")]
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use atomic_refcell::{AtomicRefCell, AtomicRefMut};
use crossbeam_channel::{Receiver, Sender};
use smallvec::SmallVec;
use thread_local::ThreadLocal;

use super::gpu::{self, CommandBuffer as _, CommandPool as _, Queue as _};
use super::{CommandBuffer, *};

const QUERY_COUNT: u32 = 1024;
const FRAME_COUNT: usize = 5;

pub struct GraphicsContext {
    device: Arc<active_gpu_backend::Device>,
    memory_allocator: Arc<MemoryAllocator>,
    fence: Arc<super::Fence>,
    current_unsubmitted_fence_value: u64,
    current_frame: u64,
    completed_frame: u64,
    frame_finished_counter_values: [u64; FRAME_COUNT],
    thread_contexts: ManuallyDrop<ThreadLocal<ThreadContext>>,
    prerendered_frames: u32,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer>>,
    global_buffer_allocator: Arc<BufferAllocator>,

    #[cfg(target_arch = "wasm32")]
    _p: PhantomData<*const u8>, // Remove Send + Sync
}

pub struct ThreadContext {
    frames: AtomicRefCell<SmallVec<[FrameContext; FRAME_COUNT]>>,
}

pub struct FrameContext {
    device: Arc<active_gpu_backend::Device>,
    command_pool: FrameContextCommandPool,
    transient_buffer_allocator: TransientBufferAllocator,
    global_buffer_allocator: Arc<BufferAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    pub(super) acceleration_structure_scratch: Option<TransientBufferSlice>,
    pub(super) acceleration_structure_scratch_offset: u64,
    frame: u64,
    query_allocator: QueryAllocator,
    remaining_command_buffers: Arc<AtomicU64>,
}

pub struct FrameContextCommandBufferEntry(Arc<AtomicU64>);

impl Drop for FrameContextCommandBufferEntry {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct FrameContextCommandPool {
    command_pool: active_gpu_backend::CommandPool,
    sender: Sender<active_gpu_backend::CommandBuffer>,
    receiver: Receiver<active_gpu_backend::CommandBuffer>,
    existing_cmd_buffer_handles: VecDeque<active_gpu_backend::CommandBuffer>,
}

impl GraphicsContext {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        memory_allocator: &Arc<MemoryAllocator>,
        buffer_allocator: &Arc<BufferAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        prerendered_frames: u32,
    ) -> Self {
        assert!(prerendered_frames <= FRAME_COUNT as u32);
        Self {
            device: device.clone(),
            memory_allocator: memory_allocator.clone(),
            destroyer: ManuallyDrop::new(destroyer.clone()),
            fence: Arc::new(super::Fence::new(device, destroyer)),
            current_unsubmitted_fence_value: 1u64,
            current_frame: 0u64,
            completed_frame: 0u64,
            frame_finished_counter_values: [1u64; 5],
            thread_contexts: ManuallyDrop::new(ThreadLocal::new()),
            prerendered_frames,
            global_buffer_allocator: buffer_allocator.clone(),

            #[cfg(target_arch = "wasm32")]
            _p: PhantomData,
        }
    }

    pub fn begin_frame(&mut self) -> u64 {
        self.current_frame += 1;
        let new_frame = self.current_frame;

        if new_frame >= self.frame_finished_counter_values.len() as u64 {
            let counter = self.frame_finished_counter_values
                [(new_frame as usize) % self.frame_finished_counter_values.len()];
            log::warn!(
                "Waiting for semaphore: {:?}, current frame: {:?}",
                counter,
                new_frame
            );
            self.fence.await_value(counter);
            self.destroyer.destroy_unused(counter);
            self.global_buffer_allocator.cleanup_unused();
            self.memory_allocator.cleanup_unused();
        }

        for thread_context in &mut (*self.thread_contexts) {
            let frame_context = thread_context.get_frame_mut(self.current_frame);
            assert_eq!(
                frame_context
                    .remaining_command_buffers
                    .load(Ordering::SeqCst),
                0
            );

            frame_context.acceleration_structure_scratch = None;
            frame_context.acceleration_structure_scratch_offset = 0;
            frame_context.frame = new_frame;

            unsafe {
                frame_context.command_pool.command_pool.reset();
            }
            frame_context.transient_buffer_allocator.reset();

            frame_context.query_allocator.reset();

            while let Ok(mut existing_cmd_buffer) = frame_context.command_pool.receiver.try_recv() {
                unsafe {
                    existing_cmd_buffer.reset(self.current_frame);
                }
                frame_context
                    .command_pool
                    .existing_cmd_buffer_handles
                    .push_back(existing_cmd_buffer);
            }
        }
        new_frame
    }

    // After calling this, it must be submitted!
    // TODO: Consider moving submission into the context.
    pub fn increment_timeline(&mut self) -> SharedFenceValuePairRef<'_> {
        let unsubmitted_fence_value = self.current_unsubmitted_fence_value;
        self.current_unsubmitted_fence_value += 1;
        self.destroyer
            .set_counter(self.current_unsubmitted_fence_value);
        self.destroyer.destroy_unused(self.fence.value());
        SharedFenceValuePairRef {
            fence: &self.fence,
            value: unsubmitted_fence_value,
            sync_before: BarrierSync::all(),
        }
    }

    pub fn end_frame(&mut self) -> SharedFenceValuePairRef<'_> {
        assert_eq!(self.current_frame, self.completed_frame + 1);
        let frame_completed_fence_value = self.current_unsubmitted_fence_value;
        self.frame_finished_counter_values
            [(self.current_frame as usize) % self.frame_finished_counter_values.len()] =
            frame_completed_fence_value;
        self.completed_frame += 1;
        self.current_unsubmitted_fence_value += 1;
        self.destroyer
            .set_counter(self.current_unsubmitted_fence_value);
        self.destroyer.destroy_unused(self.fence.value());
        log::warn!(
            "Ending frame: {}, with counter value: {}",
            self.current_frame,
            self.current_unsubmitted_fence_value - 1
        );
        SharedFenceValuePairRef {
            fence: &self.fence,
            value: frame_completed_fence_value,
            sync_before: BarrierSync::all(),
        }
    }

    pub fn get_command_buffer(&self, _queue_type: QueueType) -> CommandBuffer<'_> {
        let thread_context = self.get_thread_context();
        let mut frame_context = thread_context.get_frame(self.current_frame);

        let existing_cmd_buffer_handle = frame_context
            .command_pool
            .existing_cmd_buffer_handles
            .pop_front();
        let cmd_buffer = existing_cmd_buffer_handle.unwrap_or_else(|| unsafe {
            frame_context
                .command_pool
                .command_pool
                .create_command_buffer()
        });

        let counter = frame_context.remaining_command_buffers.clone();
        counter.fetch_add(1, Ordering::SeqCst);
        let frame_context_entry = FrameContextCommandBufferEntry(counter);

        let mut recorder = CommandBuffer::new(self, frame_context, cmd_buffer, frame_context_entry);
        recorder.begin(self.current_frame);
        recorder
    }

    pub(super) fn get_thread_frame_context(&self, frame: u64) -> AtomicRefMut<'_, FrameContext> {
        let thread_context = self.get_thread_context();
        thread_context.get_frame(frame)
    }

    fn get_thread_context(&self) -> &ThreadContext {
        self.thread_contexts.get_or(|| {
            ThreadContext::new(
                &self.device,
                &self.global_buffer_allocator,
                &self.memory_allocator,
                &self.destroyer,
                self.prerendered_frames,
            )
        })
    }

    #[inline(always)]
    pub fn prerendered_frames(&self) -> u32 {
        self.prerendered_frames
    }
}

impl Drop for GraphicsContext {
    fn drop(&mut self) {
        if self.current_frame > 0 {
            let counter = self.current_unsubmitted_fence_value - 1;
            self.fence.await_value(counter);
            self.destroyer
                .destroy_unused(counter.max(self.fence.value()));
        }

        unsafe { ManuallyDrop::drop(&mut self.thread_contexts) };
        unsafe { ManuallyDrop::drop(&mut self.destroyer) };
    }
}

// ThreadContext is only ever accessed through GraphicsContext.
// GraphicsContext will be turned !Send + !Sync on Wasm32, so we can make ThreadContext Send + Sync
// so ThreadLocal is fine with it.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for ThreadContext {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for ThreadContext {}

impl ThreadContext {
    fn new(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        prerendered_frames: u32,
    ) -> Self {
        let mut frames = SmallVec::<[FrameContext; 5]>::with_capacity(prerendered_frames as usize);
        for _ in 0..prerendered_frames {
            frames.push(FrameContext::new(
                device,
                buffer_allocator,
                memory_allocator,
                destroyer,
            ));
        }

        Self {
            frames: AtomicRefCell::new(frames),
        }
    }

    pub fn get_frame(&self, frame_counter: u64) -> AtomicRefMut<'_, FrameContext> {
        let frames = self.frames.borrow_mut();
        AtomicRefMut::map(frames, |f| {
            let len = f.len();
            &mut f[(frame_counter as usize) % len]
        })
    }

    pub fn get_frame_mut(&mut self, frame_counter: u64) -> &mut FrameContext {
        let frames = self.frames.get_mut();
        let len = frames.len();
        &mut frames[(frame_counter as usize) % len]
    }
}

impl FrameContext {
    fn new(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
    ) -> Self {
        let command_pool = unsafe {
            device
                .graphics_queue()
                .create_command_pool(gpu::CommandPoolFlags::empty())
        };
        let (sender, receiver) =
            crossbeam_channel::unbounded::<active_gpu_backend::CommandBuffer>();
        let transient_buffer_allocator = TransientBufferAllocator::new(
            device,
            memory_allocator,
            destroyer,
            memory_allocator.is_uma(),
        );
        Self {
            device: device.clone(),
            command_pool: FrameContextCommandPool {
                command_pool,
                sender,
                receiver,
                existing_cmd_buffer_handles: VecDeque::new(),
            },
            transient_buffer_allocator,
            global_buffer_allocator: buffer_allocator.clone(),
            destroyer: destroyer.clone(),
            acceleration_structure_scratch: None,
            acceleration_structure_scratch_offset: 0u64,
            frame: 1u64,
            query_allocator: QueryAllocator::new(device, destroyer, QUERY_COUNT),
            remaining_command_buffers: Arc::new(AtomicU64::new(0u64)),
        }
    }

    #[inline(always)]
    pub(super) fn transient_buffer_allocator(&self) -> &TransientBufferAllocator {
        &self.transient_buffer_allocator
    }

    #[inline(always)]
    pub(super) fn global_buffer_allocator(&self) -> &BufferAllocator {
        &self.global_buffer_allocator
    }

    #[inline(always)]
    pub(super) fn destroyer(&self) -> &Arc<DeferredDestroyer> {
        &self.destroyer
    }

    #[inline(always)]
    pub(super) fn device(&self) -> &Arc<active_gpu_backend::Device> {
        &self.device
    }

    #[inline(always)]
    pub(super) fn frame(&self) -> u64 {
        self.frame
    }

    #[inline(always)]
    pub(super) fn query_allocator(&mut self) -> &mut QueryAllocator {
        &mut self.query_allocator
    }

    #[inline(always)]
    pub(super) fn sender(&self) -> &Sender<active_gpu_backend::CommandBuffer> {
        &self.command_pool.sender
    }
}
