use std::sync::{Arc, Mutex};

use sourcerenderer_core::gpu::*;

use super::*;

pub(super) struct DeferredDestroyer<B: GPUBackend> {
    inner: Mutex<DeferredDestroyerInner<B>>
  }

  struct DeferredDestroyerInner<B: GPUBackend> {
    current_counter: u64,
    allocations: Vec<(u64, MemoryAllocation<B::Heap>)>,
    textures: Vec<(u64, B::Texture)>,
    texture_refs: Vec<(u64, Arc<super::Texture<B>>)>,
    texture_views: Vec<(u64, B::TextureView)>,
    buffers: Vec<(u64, B::Buffer)>,
    buffer_refs: Vec<(u64, Arc<B::Buffer>)>,
    buffer_slice_refs: Vec<(u64, Arc<BufferSlice<B>>)>
  }

  impl<B: GPUBackend> DeferredDestroyer<B> {
      pub(crate) fn new() -> Self {
          Self {
              inner: Mutex::new(
                  DeferredDestroyerInner {
                      current_counter: 0u64,
                      allocations: Vec::new(),
                      textures: Vec::new(),
                      texture_refs: Vec::new(),
                      texture_views: Vec::new(),
                      buffers: Vec::new(),
                      buffer_refs: Vec::new(),
                      buffer_slice_refs: Vec::new()
                  }
              )
          }
      }

      pub fn destroy_allocation(&self, allocation: MemoryAllocation<B::Heap>) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.allocations.push((frame, allocation));
      }

      pub fn destroy_texture(&self, texture: B::Texture) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.textures.push((frame, texture));
      }

      pub fn destroy_texture_reference(&self, texture: Arc<super::Texture<B>>) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.texture_refs.push((frame, texture));
      }

      pub fn destroy_texture_view(&self, texture_view: B::TextureView) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.texture_views.push((frame, texture_view));
      }

      pub fn destroy_buffer(&self, buffer: B::Buffer) {
          let mut guard: std::sync::MutexGuard<'_, DeferredDestroyerInner<B>> = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.buffers.push((frame, buffer));
      }

      pub fn destroy_buffer_reference(&self, buffer: Arc<B::Buffer>) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.buffer_refs.push((frame, buffer));
      }

      pub fn destroy_buffer_slice_reference(&self, buffer: Arc<BufferSlice<B>>) {
          let mut guard = self.inner.lock().unwrap();
          let frame = guard.current_counter;
          guard.buffer_slice_refs.push((frame, buffer));
      }

      pub fn set_counter(&self, counter: u64) {
          let mut guard = self.inner.lock().unwrap();
          guard.current_counter = counter;
      }

      pub fn destroy_unused(&self, counter: u64) {
          let mut guard = self.inner.lock().unwrap();
          guard.textures.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.texture_refs.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.texture_views.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.buffers.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.buffer_refs.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.buffer_slice_refs.retain(|(resource_counter, _)| *resource_counter > counter);
          guard.allocations.retain(|(resource_counter, _)| *resource_counter > counter);
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
