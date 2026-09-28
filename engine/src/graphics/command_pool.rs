use std::mem::ManuallyDrop;
use std::sync::Arc;

use sourcerenderer_core::gpu::{self, CommandPool as _, Queue as _};

use super::{
    BufferAllocator, DeferredDestroyer, MemoryAllocator, QueryAllocator, TransientBufferAllocator,
    TransientBufferSlice, active_gpu_backend,
};

const QUERY_COUNT: u32 = 1024;

pub(super) struct CommandPool {
    device: Arc<active_gpu_backend::Device>,
    command_pool: ManuallyDrop<active_gpu_backend::CommandPool>,
    transient_buffer_allocator: TransientBufferAllocator,
    global_buffer_allocator: Arc<BufferAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    pub(super) acceleration_structure_scratch: Option<TransientBufferSlice>,
    pub(super) acceleration_structure_scratch_offset: u64,
    generation: u64,
    query_allocator: QueryAllocator,
}

impl CommandPool {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        buffer_allocator: &Arc<BufferAllocator>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        name: Option<&str>,
    ) -> Self {
        let command_pool = unsafe {
            device
                .graphics_queue()
                .create_command_pool(gpu::CommandPoolFlags::empty(), name)
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
            generation: 1u64,
            query_allocator: QueryAllocator::new(device, destroyer, QUERY_COUNT),
        }
    }

    pub(super) unsafe fn reset(&mut self, generation: u64) {
        self.acceleration_structure_scratch = None;
        self.acceleration_structure_scratch_offset = 0;
        self.generation = generation;
        unsafe {
            self.command_pool.reset();
        }
        self.transient_buffer_allocator.reset();
        self.query_allocator.reset();
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
        self.generation
    }

    #[inline(always)]
    pub(super) fn query_allocator(&mut self) -> &mut QueryAllocator {
        &mut self.query_allocator
    }

    #[inline(always)]
    pub(super) fn command_pool(&mut self) -> &mut active_gpu_backend::CommandPool {
        &mut self.command_pool
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        let cmd_pool = unsafe { ManuallyDrop::take(&mut self.command_pool) };
        self.destroyer.destroy_command_pool(cmd_pool);
    }
}
