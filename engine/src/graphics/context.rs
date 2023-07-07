use std::{sync::{Arc, Mutex}, mem::ManuallyDrop};

use smallvec::SmallVec;
use thread_local::ThreadLocal;

use sourcerenderer_core::gpu::*;

use super::*;

pub struct GPUContext<B: GPUBackend> {
  device: Arc<B::Device>,
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
  command_pool: B::CommandPool
}

impl<B: GPUBackend> GPUContext<B> {
  pub(crate) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, prerendered_frames: u32) -> Self {
    Self {
      device: device.clone(),
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
      self.fence.await_value(recycled_frame);
      self.destroyer.destroy_unused(recycled_frame);
    }
  }

  pub fn get_thread_context(&self) -> &ThreadContext<B> {
    self.thread_contexts.get_or(|| ThreadContext::new(&self.device, self.prerendered_frames))
  }
}

impl<B: GPUBackend> Drop for GPUContext<B> {
  fn drop(&mut self) {
      self.fence.await_value(self.current_frame - 1);

      unsafe { ManuallyDrop::drop(&mut self.thread_contexts) };

      // Buffer slices can
      assert_eq!(Arc::strong_count(&self.destroyer), 1);

      unsafe { ManuallyDrop::drop(&mut self.destroyer) };
  }
}

impl<B: GPUBackend> ThreadContext<B> {
  pub fn new(device: &Arc<B::Device>, prerendered_frames: u32) -> Self {
    let mut frames = SmallVec::<[FrameContext<B>; 5]>::with_capacity(prerendered_frames as usize);
    for _ in 0..prerendered_frames {
      frames.push(FrameContext::new(device));
    }

    Self {
      device: device.clone(),
      frames,
    }
  }
}

impl<B: GPUBackend> FrameContext<B> {
  pub fn new(device: &Arc<B::Device>) -> Self {
    let command_pool = unsafe { device.graphics_queue().create_command_pool(CommandPoolType::CommandBuffers) };
    Self {
      device: device.clone(),
      command_pool
    }
  }
}

pub(crate) struct DeferredDestroyer<B: GPUBackend> {
  inner: Mutex<DeferredDestroyerInner<B>>
}

struct DeferredDestroyerInner<B: GPUBackend> {
  current_counter: u64,
  textures: Vec<(u64, B::Texture)>,
  texture_views: Vec<(u64, B::TextureView)>,
  buffers: Vec<(u64, B::Buffer)>,
  buffer_refs: Vec<(u64, Arc<B::Buffer>)>
}

impl<B: GPUBackend> DeferredDestroyer<B> {
  pub fn destroy_texture(&self, texture: B::Texture) {
    let mut guard = self.inner.lock().unwrap();
    let frame = guard.current_counter;
    guard.textures.push((frame, texture));
  }

  pub fn destroy_texture_view(&self, texture_view: B::TextureView) {
    let mut guard = self.inner.lock().unwrap();
    let frame = guard.current_counter;
    guard.texture_views.push((frame, texture_view));
  }

  pub fn destroy_buffer_reference(&self, buffer: Arc<B::Buffer>) {
    let mut guard = self.inner.lock().unwrap();
    let frame = guard.current_counter;
    guard.buffer_refs.push((frame, buffer));
  }

  pub fn destroy_buffer(&self, buffer: B::Buffer) {
    let mut guard: std::sync::MutexGuard<'_, DeferredDestroyerInner<B>> = self.inner.lock().unwrap();
    let frame = guard.current_counter;
    guard.buffers.push((frame, buffer));
  }

  pub fn set_counter(&self, counter: u64) {
    let mut guard = self.inner.lock().unwrap();
    guard.current_counter = counter;
  }

  pub fn destroy_unused(&self, counter: u64) {
    let mut guard = self.inner.lock().unwrap();
    guard.textures.retain(|(resource_counter, _texture)| *resource_counter > counter);
    guard.texture_views.retain(|(resource_counter, _texture)| *resource_counter > counter);
    guard.buffers.retain(|(resource_counter, _texture)| *resource_counter > counter);
    guard.buffer_refs.retain(|(resource_counter, _texture)| *resource_counter > counter);
  }
}

impl<B: GPUBackend> Drop for DeferredDestroyer<B> {
  fn drop(&mut self) {
    let guard = self.inner.lock().unwrap();
      assert!(guard.textures.is_empty());
      assert!(guard.texture_views.is_empty());
      assert!(guard.buffer_refs.is_empty());
      assert!(guard.buffers.is_empty());
  }
}
