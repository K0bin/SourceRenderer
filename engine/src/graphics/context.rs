use std::mem::ManuallyDrop;
use std::sync::Arc;

use atomic_refcell::AtomicRefCell;
use bevy_tasks::ComputeTaskPool;
use smallvec::SmallVec;
use thread_local::ThreadLocal;

use super::{CommandBuffer, *};

const FRAME_COUNT: usize = 5;

pub struct GraphicsContext {
    device: Arc<Device>,
    memory_allocator: Arc<MemoryAllocator>,
    current_frame: u64,
    completed_frame: u64,
    frame_finished_counter_values: [SmallVec<[QueueFenceValue; 3]>; FRAME_COUNT],
    thread_frames: ManuallyDrop<ThreadLocal<ThreadFrames>>,
    prerendered_frames: u32,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer>>,
    global_buffer_allocator: Arc<BufferAllocator>,
}

struct ThreadFrames(SmallVec<[FrameContext; FRAME_COUNT]>);

pub struct FrameContext {
    command_pool: Arc<AtomicRefCell<CommandPool>>,
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
            frame_finished_counter_values: Default::default(),
            thread_frames: ManuallyDrop::new(ThreadLocal::new()),
            prerendered_frames,
            global_buffer_allocator: buffer_allocator.clone(),
        }
    }

    fn new_thread_frame(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        prerendered_frames: u32,
    ) -> ThreadFrames {
        let mut frames =
            SmallVec::<[FrameContext; FRAME_COUNT]>::with_capacity(prerendered_frames as usize);
        for i in 0..prerendered_frames {
            frames.push(FrameContext::new(
                device,
                buffer_allocator,
                memory_allocator,
                destroyer,
                i,
            ));
        }
        ThreadFrames(frames)
    }

    pub fn begin_frame(&mut self) -> u64 {
        self.current_frame += 1;
        let new_frame = self.current_frame;

        if new_frame >= self.prerendered_frames as u64 {
            let counters = &self.frame_finished_counter_values
                [(new_frame as usize) % (self.prerendered_frames as usize)];
            self.destroyer.set_counter(new_frame);
            for &(queue_type, counter) in counters {
                if counter > 0 {
                    self.device.await_queue_counter(queue_type, counter);
                }
            }
            self.destroyer
                .destroy_unused(new_frame - (self.prerendered_frames as u64));
            self.global_buffer_allocator.cleanup_unused();
            self.memory_allocator.cleanup_unused();
        }

        for thread_frame in &mut (*self.thread_frames) {
            let frames_len = thread_frame.0.len();
            let frame = &mut thread_frame.0[(new_frame as usize) % frames_len];
            let mut command_pool = frame.command_pool.borrow_mut();
            unsafe {
                command_pool.reset(new_frame);
            }
        }
        new_frame
    }

    pub fn end_frame(&mut self) {
        assert_eq!(self.current_frame, self.completed_frame + 1);

        // Flush all transient buffers
        // - Makes Vulkan non-coherent memory of the buffer available to the device.
        // - On WebGPU it does the actual copy. Doing one big copy here means less work for the
        //   WebGPU implementation and only one GPU copy per suballocated buffer.
        for thread_frame in &mut (*self.thread_frames) {
            let frames_len = thread_frame.0.len();
            let frame = &mut thread_frame.0[(self.current_frame as usize) % frames_len];
            let command_pool = frame.command_pool.borrow_mut();
            command_pool.transient_buffer_allocator().flush_all();
        }

        let mut fences = SmallVec::<[QueueFenceValue; 3]>::new();
        fences.push((
            QueueType::Graphics,
            self.device.submitted_queue_counter(QueueType::Graphics),
        ));
        if self.device.has_queue(QueueType::Compute) {
            fences.push((
                QueueType::Compute,
                self.device.submitted_queue_counter(QueueType::Compute),
            ));
        }
        if self.device.has_queue(QueueType::Transfer) {
            fences.push((
                QueueType::Transfer,
                self.device.submitted_queue_counter(QueueType::Transfer),
            ));
        }
        self.frame_finished_counter_values
            [(self.current_frame as usize) % (self.prerendered_frames as usize)] = fences;
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
        let result: Vec<FinishedCommandBuffer>;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pool = ComputeTaskPool::get();
            result = pool.scope(|s| {
                for element in elements {
                    s.spawn(async {
                        let mut cmd_buffer = self.get_command_buffer(queue_type);
                        callback(&mut cmd_buffer, element);
                        cmd_buffer.finish()
                    })
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            result = elements
                .iter()
                .map(|element| {
                    let mut cmd_buffer = self.get_command_buffer(queue_type);
                    callback(&mut cmd_buffer, element);
                    cmd_buffer.finish()
                })
                .collect();
        }

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
        let command_pool = frame_context.command_pool.borrow_mut();

        let mut cmd_buffer = CommandBuffer::new(
            command_pool,
            &self.destroyer,
            queue_type,
            Some(&format!("Cmd Buffer for frame {}", self.current_frame)),
        );
        cmd_buffer.begin();
        cmd_buffer
    }

    pub fn get_thread_frame_context(&self, frame: u64) -> &FrameContext {
        let thread_frames = self.get_thread_frames();
        let len = thread_frames.0.len();
        &thread_frames.0[(frame as usize) % len]
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
            self.destroyer.destroy_unused(self.current_frame);
        }

        unsafe { ManuallyDrop::drop(&mut self.thread_frames) };
        unsafe { ManuallyDrop::drop(&mut self.destroyer) };
    }
}

// ThreadFrames is only ever accessed through GraphicsContext.
// GraphicsContext will be turned !Send + !Sync on Wasm32 by holding the Device,
// so we can make ThreadFrames Send + Sync to make ThreadLocal happy.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for ThreadFrames {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for ThreadFrames {}

impl FrameContext {
    fn new(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        context_idx: u32,
    ) -> Self {
        Self {
            command_pool: Arc::new(AtomicRefCell::new(CommandPool::new(
                device,
                buffer_allocator,
                memory_allocator,
                destroyer,
                Some(&format!("Cmd Pool context {}", context_idx)),
            ))),
        }
    }
}
