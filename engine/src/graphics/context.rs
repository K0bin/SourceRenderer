use std::{sync::{Arc, Mutex}, mem::ManuallyDrop};

use smallvec::SmallVec;
use thread_local::ThreadLocal;

use sourcerenderer_core::gpu::*;

use super::*;

pub struct GraphicsContext<B: GPUBackend> {
  device: Arc<B::Device>,
  allocator: Arc<MemoryAllocator<B>>,
  fence: B::Fence,
  current_frame: u64,
  thread_contexts: ManuallyDrop<ThreadLocal<ThreadContext<B>>>,
  prerendered_frames: u32,
  destroyer: ManuallyDrop<Arc<DeferredDestroyer<B>>>
}

pub struct ThreadContext<B: GPUBackend> {
  device: Arc<B::Device>,
  frames: SmallVec<[FrameContext<B>; 5]>
}

pub struct FrameContext<B: GPUBackend> {
  device: Arc<B::Device>,
  command_pool: B::CommandPool,
  buffer_allocator: TransientBufferAllocator<B>
}

impl<B: GPUBackend> GraphicsContext<B> {
  pub(super) fn new(device: &Arc<B::Device>, allocator: &Arc<MemoryAllocator<B>>, destroyer: &Arc<DeferredDestroyer<B>>, prerendered_frames: u32) -> Self {
    Self {
      device: device.clone(),
      allocator: allocator.clone(),
      destroyer: ManuallyDrop::new(destroyer.clone()),
      fence: unsafe { device.create_fence() },
      current_frame: 0u64,
      thread_contexts: ManuallyDrop::new(ThreadLocal::new()),
      prerendered_frames
    }
  }

  pub fn begin_frame(&mut self) {
    self.current_frame += 1;
    let new_frame = self.current_frame;
    self.destroyer.set_counter(new_frame);

    if new_frame > self.prerendered_frames as u64 {
      let recycled_frame = new_frame - self.prerendered_frames as u64;
      unsafe { self.fence.await_value(recycled_frame); }
      self.destroyer.destroy_unused(recycled_frame);
    }
  }

  pub fn get_command_buffer(&self, command_buffer_type: CommandBufferType) {
    let thread_context = self.get_thread_context();
    let frame_context = thread_context.get_frame(self.current_frame);
  }

  fn get_thread_context(&self) -> &ThreadContext<B> {
    self.thread_contexts.get_or(|| ThreadContext::new(&self.device, &self.allocator, &self.destroyer, self.prerendered_frames))
  }
}

impl<B: GPUBackend> Drop for GraphicsContext<B> {
    fn drop(&mut self) {
        unsafe {
            self.fence.await_value(self.current_frame - 1);
        }

        unsafe { ManuallyDrop::drop(&mut self.thread_contexts) };
        assert_eq!(Arc::strong_count(&self.destroyer), 1);
        unsafe { ManuallyDrop::drop(&mut self.destroyer) };
    }
}

impl<B: GPUBackend> ThreadContext<B> {
  fn new(device: &Arc<B::Device>, memory_allocator: &Arc<MemoryAllocator<B>>, destroyer: &Arc<DeferredDestroyer<B>>, prerendered_frames: u32) -> Self {
    let mut frames = SmallVec::<[FrameContext<B>; 5]>::with_capacity(prerendered_frames as usize);
    for _ in 0..prerendered_frames {
      frames.push(FrameContext::new(device, memory_allocator, destroyer));
    }

    Self {
      device: device.clone(),
      frames,
    }
  }

  pub fn get_frame(&self, frame_counter: u64) -> &FrameContext<B> {
    &self.frames[(frame_counter as usize) % self.frames.len()]
  }
}

impl<B: GPUBackend> FrameContext<B> {
  fn new(device: &Arc<B::Device>, memory_allocator: &Arc<MemoryAllocator<B>>, destroyer: &Arc<DeferredDestroyer<B>>) -> Self {
    let command_pool = unsafe { device.graphics_queue().create_command_pool(CommandPoolType::CommandBuffers, CommandPoolFlags::empty()) };
    let buffer_allocator = TransientBufferAllocator::new(device, memory_allocator, destroyer);
    Self {
      device: device.clone(),
      command_pool,
      buffer_allocator,
    }
  }
}
