#[cfg(target_arch = "wasm32")]
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use atomic_refcell::{AtomicRefCell, AtomicRefMut};
use bevy_tasks::ComputeTaskPool;
use smallvec::SmallVec;
use thread_local::ThreadLocal;

use super::gpu::{self, CommandPool as _, Queue as _};
use super::{CommandBuffer, *};

const QUERY_COUNT: u32 = 1024;
const FRAME_COUNT: usize = 5;

pub struct GraphicsContext {
    device: Arc<Device>,
    memory_allocator: Arc<MemoryAllocator>,
    current_frame: u64,
    completed_frame: u64,
    frame_finished_counter_values: [u64; FRAME_COUNT],
    thread_frames: ManuallyDrop<ThreadLocal<ThreadFrames>>,
    prerendered_frames: u32,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer>>,
    global_buffer_allocator: Arc<BufferAllocator>,

    #[cfg(target_arch = "wasm32")]
    _p: PhantomData<*const u8>, // Remove Send + Sync
}

type ThreadFrames = AtomicRefCell<SmallVec<[FrameContext; FRAME_COUNT]>>;

pub struct FrameContext {
    device: Arc<active_gpu_backend::Device>,
    pub(super) command_pool: ManuallyDrop<active_gpu_backend::CommandPool>,
    transient_buffer_allocator: TransientBufferAllocator,
    global_buffer_allocator: Arc<BufferAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    pub(super) acceleration_structure_scratch: Option<TransientBufferSlice>,
    pub(super) acceleration_structure_scratch_offset: u64,
    frame: u64,
    query_allocator: QueryAllocator,
}

impl GraphicsContext {
    pub(super) fn new(
        device: &Arc<Device>,
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
            current_frame: 0u64,
            completed_frame: 0u64,
            frame_finished_counter_values: [1u64; 5],
            thread_frames: ManuallyDrop::new(ThreadLocal::new()),
            prerendered_frames,
            global_buffer_allocator: buffer_allocator.clone(),

            #[cfg(target_arch = "wasm32")]
            _p: PhantomData,
        }
    }

    fn new_thread_frame(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        prerendered_frames: u32,
    ) -> ThreadFrames {
        let mut frames = SmallVec::<[FrameContext; 5]>::with_capacity(prerendered_frames as usize);
        for _ in 0..prerendered_frames {
            frames.push(FrameContext::new(
                device,
                buffer_allocator,
                memory_allocator,
                destroyer,
            ));
        }
        AtomicRefCell::new(frames)
    }

    pub fn begin_frame(&mut self) -> u64 {
        self.current_frame += 1;
        let new_frame = self.current_frame;

        if new_frame >= self.frame_finished_counter_values.len() as u64 {
            let counter = self.frame_finished_counter_values
                [(new_frame as usize) % self.frame_finished_counter_values.len()];
            self.device.await_counter(counter);
            self.global_buffer_allocator.cleanup_unused();
            self.memory_allocator.cleanup_unused();
        }

        for thread_frame in &mut (*self.thread_frames) {
            let mut frames = thread_frame.borrow_mut();
            let frames_len = frames.len();
            let frame = &mut frames[(self.current_frame as usize) % frames_len];

            frame.acceleration_structure_scratch = None;
            frame.acceleration_structure_scratch_offset = 0;
            frame.frame = new_frame;

            unsafe {
                frame.command_pool.reset();
            }
            frame.transient_buffer_allocator.reset();

            frame.query_allocator.reset();
        }
        new_frame
    }

    pub fn end_frame(&mut self) {
        assert_eq!(self.current_frame, self.completed_frame + 1);
        let frame_completed_fence_value = self.device.queue_next_counter(QueueType::Graphics);
        self.frame_finished_counter_values
            [(self.current_frame as usize) % self.frame_finished_counter_values.len()] =
            frame_completed_fence_value;
        self.completed_frame += 1;
    }

    pub fn build_waits(
        &self,
        submit_queue: QueueType,
        wait_for_graphics: Option<u64>,
        wait_for_compute: Option<u64>,
        wait_for_transfer: Option<u64>,
    ) -> SmallVec<[QueueFenceValue; 2]> {
        let mut wait_fences: SmallVec<[QueueFenceValue; 2]> = SmallVec::new();
        if let Some(wait) = wait_for_graphics {
            if submit_queue == QueueType::Graphics {
                panic!(
                    "Cannot wait for the queue the work is going to be submitted to. Use barriers instead."
                );
            }
            wait_fences.push((QueueType::Graphics, wait));
        }
        if let Some(wait) = wait_for_compute {
            if submit_queue == QueueType::Compute {
                panic!(
                    "Cannot wait for the queue the work is going to be submitted to. Use barriers instead."
                );
            }
            assert!(self.device.has_queue(QueueType::Compute));
            wait_fences.push((QueueType::Compute, wait));
        }
        if let Some(wait) = wait_for_transfer {
            if submit_queue == QueueType::Transfer {
                panic!(
                    "Cannot wait for the queue the work is going to be submitted to. Use barriers instead."
                );
            }
            assert!(self.device.has_queue(QueueType::Transfer));
            wait_fences.push((QueueType::Transfer, wait));
        }
        wait_fences
    }

    pub fn with_par_command_buffers<'a, T: Sync, F>(
        &self,
        queue_type: QueueType,
        elements: &[T],
        callback: F,
        wait_for_graphics: Option<u64>,
        wait_for_compute: Option<u64>,
        wait_for_transfer: Option<u64>,
    ) where
        for<'b> F: Fn(&mut CommandBuffer<'b>, &T) -> FinishedCommandBuffer,
        F: Sync,
    {
        let pool = ComputeTaskPool::get();
        let result = pool.scope(|s| {
            for element in elements {
                s.spawn(async {
                    let mut cmd_buffer = self.get_command_buffer(queue_type);
                    callback(&mut cmd_buffer, element);
                    cmd_buffer.finish()
                })
            }
        });
        let wait_fences = self.build_waits(
            queue_type,
            wait_for_graphics,
            wait_for_compute,
            wait_for_transfer,
        );

        for fence in wait_fences {
            self.device
                .wait_for(queue_type, fence.0, fence.1, BarrierSync::all());
        }

        for cmd_buffer in result {
            self.device.submit(queue_type, cmd_buffer);
        }
    }

    pub fn with_command_buffer<'a, T: Sync, F>(
        &self,
        queue_type: QueueType,
        callback: F,
        wait_for_graphics: Option<u64>,
        wait_for_compute: Option<u64>,
        wait_for_transfer: Option<u64>,
    ) where
        for<'b> F: FnOnce(&mut CommandBuffer<'b>) -> FinishedCommandBuffer,
        F: Sync,
    {
        let mut cmd_buffer = self.get_command_buffer(queue_type);
        callback(&mut cmd_buffer);
        let cmd_buffer = cmd_buffer.finish();

        let wait_fences = self.build_waits(
            queue_type,
            wait_for_graphics,
            wait_for_compute,
            wait_for_transfer,
        );

        for fence in wait_fences {
            self.device
                .wait_for(queue_type, fence.0, fence.1, BarrierSync::all());
        }

        self.device.submit(queue_type, cmd_buffer);
    }

    pub fn get_command_buffer(&self, queue_type: QueueType) -> CommandBuffer<'_> {
        let frame_context = self.get_thread_frame_context(self.current_frame);

        let mut cmd_buffer = CommandBuffer::new(self, frame_context, &self.destroyer, queue_type);
        cmd_buffer.begin();
        cmd_buffer
    }

    pub(super) fn get_thread_frame_context(&self, frame: u64) -> AtomicRefMut<'_, FrameContext> {
        let thread_frames = self.get_thread_frames();
        let frames = thread_frames.borrow_mut();
        AtomicRefMut::map(frames, |f| {
            let len = f.len();
            &mut f[(frame as usize) % len]
        })
    }

    fn get_thread_frames(&self) -> &ThreadFrames {
        self.thread_frames.get_or(|| {
            Self::new_thread_frame(
                self.device.handle(),
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
            self.device.block_until_idle();
            self.destroyer
                .destroy_unused(self.device.completed_queue_counter(QueueType::Graphics));
        }

        unsafe { ManuallyDrop::drop(&mut self.thread_frames) };
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
        let transient_buffer_allocator = TransientBufferAllocator::new(
            device,
            memory_allocator,
            destroyer,
            memory_allocator.is_uma(),
        );
        Self {
            device: device.clone(),
            command_pool: ManuallyDrop::new(command_pool),
            transient_buffer_allocator,
            global_buffer_allocator: buffer_allocator.clone(),
            destroyer: destroyer.clone(),
            acceleration_structure_scratch: None,
            acceleration_structure_scratch_offset: 0u64,
            frame: 1u64,
            query_allocator: QueryAllocator::new(device, destroyer, QUERY_COUNT),
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
}

impl Drop for FrameContext {
    fn drop(&mut self) {
        let cmd_pool = unsafe { ManuallyDrop::take(&mut self.command_pool) };
        self.destroyer.destroy_command_pool(cmd_pool);
    }
}
