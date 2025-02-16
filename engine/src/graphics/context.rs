use std::collections::VecDeque;
use std::{sync::Arc, mem::ManuallyDrop};

use crossbeam_channel::{Sender, Receiver};
use smallvec::SmallVec;
use thread_local::ThreadLocal;

use sourcerenderer_core::gpu::*;
use sourcerenderer_core::gpu;
use atomic_refcell::{AtomicRefCell, AtomicRefMut};

use super::*;
use super::CommandBuffer;

pub struct GraphicsContext<B: GPUBackend> {
  device: Arc<B::Device>,
  memory_allocator: Arc<MemoryAllocator<B>>,
  fence: Arc<super::Fence<B>>,
  current_frame: u64,
  completed_frame: u64,
  thread_contexts: ManuallyDrop<ThreadLocal<ThreadContext<B>>>,
  prerendered_frames: u32,
  destroyer: ManuallyDrop<Arc<DeferredDestroyer<B>>>,
  global_buffer_allocator: Arc<BufferAllocator<B>>,
}

pub struct ThreadContext<B: GPUBackend> {
  device: Arc<B::Device>,
  frames: AtomicRefCell<SmallVec<[FrameContext<B>; 5]>>
}

pub struct FrameContext<B: GPUBackend> {
  device: Arc<B::Device>,
  command_pool: FrameContextCommandPool<B>,
  secondary_command_pool: FrameContextCommandPool<B>,
  buffer_allocator: Arc<TransientBufferAllocator<B>>,
  last_used_frame: u64,
}

struct FrameContextCommandPool<B: GPUBackend> {
    command_pool: B::CommandPool,
    sender: Sender<Box<CommandBuffer<B>>>,
    receiver: Receiver<Box<CommandBuffer<B>>>,
    existing_cmd_buffers: VecDeque<Box<CommandBuffer<B>>>
}

impl<B: GPUBackend> GraphicsContext<B> {
  pub(super) fn new(device: &Arc<B::Device>, memory_allocator: &Arc<MemoryAllocator<B>>, buffer_allocator: &Arc<BufferAllocator<B>>, destroyer: &Arc<DeferredDestroyer<B>>, prerendered_frames: u32) -> Self {
    Self {
      device: device.clone(),
      memory_allocator: memory_allocator.clone(),
      destroyer: ManuallyDrop::new(destroyer.clone()),
      fence: Arc::new(super::Fence::<B>::new(device, destroyer)),
      current_frame: 1u64, // Fences (Timeline semaphores) start at value 0, so waiting for 0 would be pointless.
      completed_frame: 1u64,
      thread_contexts: ManuallyDrop::new(ThreadLocal::new()),
      prerendered_frames,
      global_buffer_allocator: buffer_allocator.clone(),
    }
  }

  pub fn begin_frame(&mut self) {
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
  }

  pub fn end_frame(&mut self) -> SharedFenceValuePairRef<B> {
    assert_eq!(self.current_frame, self.completed_frame + 1);
    self.completed_frame += 1;
    SharedFenceValuePairRef {
        fence: &self.fence,
        value: self.current_frame,
        sync_before: BarrierSync::all()
    }
  }

  pub fn get_command_buffer(&mut self, queue_type: QueueType) -> CommandBufferRecorder<B> {
    let thread_context = self.get_thread_context();
    let mut frame_context = thread_context.get_frame(self.current_frame);

    if frame_context.last_used_frame != self.current_frame {
        unsafe { frame_context.command_pool.command_pool.reset(); }
        unsafe { frame_context.secondary_command_pool.command_pool.reset(); }
        frame_context.buffer_allocator.reset();
        frame_context.last_used_frame = self.current_frame;

        while let Ok(mut existing_cmd_buffer) = frame_context.command_pool.receiver.try_recv() {
            existing_cmd_buffer.reset(self.current_frame);
            frame_context.command_pool.existing_cmd_buffers.push_back(existing_cmd_buffer);
        }
        while let Ok(mut existing_cmd_buffer) = frame_context.secondary_command_pool.receiver.try_recv() {
            existing_cmd_buffer.reset(self.current_frame);
            frame_context.secondary_command_pool.existing_cmd_buffers.push_back(existing_cmd_buffer);
        }
    }

    let existing_cmd_buffer = frame_context.command_pool.existing_cmd_buffers.pop_front();
    let cmd_buffer = existing_cmd_buffer.unwrap_or_else(|| {
        Box::new(CommandBuffer::new(
            unsafe { frame_context.command_pool.command_pool.create_command_buffer() },
            &self.device,
            &frame_context.buffer_allocator,
            &self.global_buffer_allocator,
            &self.destroyer
        ))
    });
    let mut recorder = CommandBufferRecorder::new(cmd_buffer, frame_context.command_pool.sender.clone());
    recorder.begin(self.current_frame, None);
    recorder
  }

  pub fn get_inner_command_buffer(&self, inheritance: &<B::CommandBuffer as gpu::CommandBuffer<B>>::CommandBufferInheritance) -> CommandBufferRecorder<B> {
    let thread_context = self.get_thread_context();
    let mut frame_context = thread_context.get_frame(self.current_frame);

    let existing_cmd_buffer = frame_context.secondary_command_pool.existing_cmd_buffers.pop_front();
    let cmd_buffer = existing_cmd_buffer.unwrap_or_else(|| {
        Box::new(CommandBuffer::new(
            unsafe { frame_context.secondary_command_pool.command_pool.create_command_buffer() },
            &self.device,
            &frame_context.buffer_allocator,
            &self.global_buffer_allocator,
            &self.destroyer
        ))
    });
    let mut recorder = CommandBufferRecorder::new(cmd_buffer, frame_context.secondary_command_pool.sender.clone());
    recorder.begin(self.current_frame, Some(inheritance));
    recorder
  }

  fn get_thread_context(&self) -> &ThreadContext<B> {
    self.thread_contexts.get_or(|| ThreadContext::new(&self.device, &self.memory_allocator, &self.destroyer, self.prerendered_frames))
  }

  pub fn prerendered_frames(&self) -> u32 {
    self.prerendered_frames
  }
}

impl<B: GPUBackend> Drop for GraphicsContext<B> {
    fn drop(&mut self) {
        if self.current_frame > 0 {
            self.fence.await_value(self.completed_frame);
            self.destroyer.destroy_unused(self.completed_frame);
        }

        unsafe { ManuallyDrop::drop(&mut self.thread_contexts) };
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
      frames: AtomicRefCell::new(frames),
    }
  }

  pub fn get_frame(&self, frame_counter: u64) -> AtomicRefMut<FrameContext<B>> {
    let frames = self.frames.borrow_mut();
    AtomicRefMut::map(frames, |f| {
        let len = f.len();
        &mut f[(frame_counter as usize) % len]
    })
  }
}

impl<B: GPUBackend> FrameContext<B> {
  fn new(device: &Arc<B::Device>, memory_allocator: &Arc<MemoryAllocator<B>>, destroyer: &Arc<DeferredDestroyer<B>>) -> Self {
    let command_pool = unsafe { device.graphics_queue().create_command_pool(CommandPoolType::CommandBuffers, CommandPoolFlags::empty()) };
    let secondary_command_pool = unsafe { device.graphics_queue().create_command_pool(CommandPoolType::InnerCommandBuffers, CommandPoolFlags::empty()) };
    let (sender, receiver) = crossbeam_channel::unbounded::<Box<CommandBuffer<B>>>();
    let (secondary_sender, secondary_receiver) = crossbeam_channel::unbounded::<Box<CommandBuffer<B>>>();
    let buffer_allocator = TransientBufferAllocator::new(device, memory_allocator, destroyer, memory_allocator.is_uma());
    Self {
      device: device.clone(),
      command_pool: FrameContextCommandPool {
        command_pool,
        sender,
        receiver,
        existing_cmd_buffers: VecDeque::new()
      },
      secondary_command_pool: FrameContextCommandPool {
        command_pool: secondary_command_pool,
        sender: secondary_sender,
        receiver: secondary_receiver,
        existing_cmd_buffers: VecDeque::new()
      },
      buffer_allocator: Arc::new(buffer_allocator),
      last_used_frame: 0u64
    }
  }
}
