use std::collections::VecDeque;
#[cfg(target_arch = "wasm32")]
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use atomic_refcell::{
    AtomicRefCell,
    AtomicRefMut,
};
use crossbeam_channel::{
    Receiver,
    Sender,
};
use smallvec::SmallVec;
use thread_local::ThreadLocal;

use super::gpu::{
    self,
    CommandBuffer as _,
    CommandPool as _,
    Queue as _,
};
use super::{
    CommandBuffer,
    *,
};

const QUERY_COUNT: u32 = 1024;

pub struct GraphicsContext {
    device: Arc<active_gpu_backend::Device>,
    memory_allocator: Arc<MemoryAllocator>,
    fence: Arc<super::Fence>,
    current_frame: u64,
    completed_frame: u64,
    thread_contexts: ManuallyDrop<ThreadLocal<ThreadContext>>,
    prerendered_frames: u32,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer>>,
    global_buffer_allocator: Arc<BufferAllocator>,

    #[cfg(target_arch = "wasm32")]
    _p: PhantomData<*const u8>, // Remove Send + Sync
}

pub struct ThreadContext {
    frames: AtomicRefCell<SmallVec<[FrameContext; 5]>>,
}

pub(super) struct FrameContext {
    device: Arc<active_gpu_backend::Device>,
    command_pool: FrameContextCommandPool,
    secondary_command_pool: FrameContextCommandPool,
    transient_buffer_allocator: TransientBufferAllocator,
    global_buffer_allocator: Arc<BufferAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    pub(super) acceleration_structure_scratch: Option<TransientBufferSlice>,
    pub(super) acceleration_structure_scratch_offset: u64,
    frame: u64,
    query_allocator: QueryAllocator,
    remaining_command_buffers: Arc<AtomicU64>,
    split_barriers: SplitBarrierPool,
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
        Self {
            device: device.clone(),
            memory_allocator: memory_allocator.clone(),
            destroyer: ManuallyDrop::new(destroyer.clone()),
            fence: Arc::new(super::Fence::new(device, destroyer)),
            current_frame: 1u64, // Fences (Timeline semaphores) start at value 0, so waiting for 0 would be pointless.
            completed_frame: 1u64,
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
        self.destroyer.set_counter(new_frame);

        if new_frame >= self.prerendered_frames as u64 {
            let recycled_frame = new_frame - self.prerendered_frames as u64;
            self.fence.await_value(recycled_frame);
            self.destroyer.destroy_unused(recycled_frame);
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
            unsafe {
                frame_context.secondary_command_pool.command_pool.reset();
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
            while let Ok(mut existing_cmd_buffer) =
                frame_context.secondary_command_pool.receiver.try_recv()
            {
                unsafe {
                    existing_cmd_buffer.reset(self.current_frame);
                }
                frame_context
                    .secondary_command_pool
                    .existing_cmd_buffer_handles
                    .push_back(existing_cmd_buffer);
            }
        }
        new_frame
    }

    pub fn end_frame(&mut self) -> SharedFenceValuePairRef {
        assert_eq!(self.current_frame, self.completed_frame + 1);
        self.completed_frame += 1;
        SharedFenceValuePairRef {
            fence: &self.fence,
            value: self.current_frame,
            sync_before: BarrierSync::all(),
        }
    }

    pub fn get_command_buffer<'a>(&'a self, _queue_type: QueueType) -> CommandBuffer<'a> {
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

        let mut recorder =
            CommandBuffer::new(self, frame_context, cmd_buffer, frame_context_entry, false);
        recorder.begin(self.current_frame, None);
        recorder
    }

    pub(super) fn get_inner_command_buffer<'a>(
        &'a self,
        inheritance: &<active_gpu_backend::CommandBuffer as gpu::CommandBuffer<
            active_gpu_backend::Backend,
        >>::CommandBufferInheritance,
    ) -> CommandBuffer<'a> {
        let thread_context = self.get_thread_context();
        let mut frame_context = thread_context.get_frame(self.current_frame);

        let existing_cmd_buffer_handle = frame_context
            .secondary_command_pool
            .existing_cmd_buffer_handles
            .pop_front();
        let cmd_buffer = existing_cmd_buffer_handle.unwrap_or_else(|| unsafe {
            frame_context
                .secondary_command_pool
                .command_pool
                .create_command_buffer()
        });

        let counter = frame_context.remaining_command_buffers.clone();
        counter.fetch_add(1, Ordering::SeqCst);
        let frame_context_entry = FrameContextCommandBufferEntry(counter);

        let mut recorder =
            CommandBuffer::new(self, frame_context, cmd_buffer, frame_context_entry, true);
        recorder.begin(self.current_frame, Some(inheritance));
        recorder
    }

    pub(super) fn get_thread_frame_context(&self, frame: u64) -> AtomicRefMut<FrameContext> {
        let thread_context = self.get_thread_context();
        thread_context.get_frame(frame)
    }

    pub fn get_split_barrier(&self) -> SplitBarrier {
        let thread_context = self.get_thread_context();
        let mut frame_context = thread_context.get_frame(self.current_frame);
        frame_context.split_barrier_pool().get_split_barrier()
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
            self.fence.await_value(self.completed_frame);
            self.destroyer.destroy_unused(self.completed_frame);
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

    pub fn get_frame(&self, frame_counter: u64) -> AtomicRefMut<FrameContext> {
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
            device.graphics_queue().create_command_pool(
                gpu::CommandPoolType::CommandBuffers,
                gpu::CommandPoolFlags::empty(),
            )
        };
        let secondary_command_pool = unsafe {
            device.graphics_queue().create_command_pool(
                gpu::CommandPoolType::InnerCommandBuffers,
                gpu::CommandPoolFlags::empty(),
            )
        };
        let (sender, receiver) =
            crossbeam_channel::unbounded::<active_gpu_backend::CommandBuffer>();
        let (secondary_sender, secondary_receiver) =
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
            secondary_command_pool: FrameContextCommandPool {
                command_pool: secondary_command_pool,
                sender: secondary_sender,
                receiver: secondary_receiver,
                existing_cmd_buffer_handles: VecDeque::new(),
            },
            transient_buffer_allocator: transient_buffer_allocator,
            global_buffer_allocator: buffer_allocator.clone(),
            destroyer: destroyer.clone(),
            acceleration_structure_scratch: None,
            acceleration_structure_scratch_offset: 0u64,
            frame: 1u64,
            query_allocator: QueryAllocator::new(device, destroyer, QUERY_COUNT),
            remaining_command_buffers: Arc::new(AtomicU64::new(0u64)),
            split_barriers: SplitBarrierPool::new(device),
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
    pub(super) fn split_barrier_pool(&mut self) -> &mut SplitBarrierPool {
        &mut self.split_barriers
    }

    #[inline(always)]
    pub(super) fn sender(&self, is_secondary: bool) -> &Sender<active_gpu_backend::CommandBuffer> {
        if !is_secondary {
            &self.command_pool.sender
        } else {
            &self.secondary_command_pool.sender
        }
    }
}
