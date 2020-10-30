use sourcerenderer_core::pool::Recyclable;
use ::{VkSemaphore, VkFence};
use std::sync::Arc;
use buffer::VkBufferSlice;
use ::{VkTexture, VkRenderPass};
use texture::VkTextureView;
use VkFrameBuffer;

pub struct VkLifetimeTrackers {
  semaphores: Vec<Arc<Recyclable<VkSemaphore>>>,
  fences: Vec<Arc<VkFence>>,
  buffers: Vec<Arc<VkBufferSlice>>,
  textures: Vec<Arc<VkTexture>>,
  texture_views: Vec<Arc<VkTextureView>>,
  render_passes: Vec<Arc<VkRenderPass>>,
  frame_buffers: Vec<Arc<VkFrameBuffer>>
}

impl VkLifetimeTrackers {
  pub(crate) fn new() -> Self {
    Self {
      semaphores: Vec::new(),
      fences: Vec::new(),
      buffers: Vec::new(),
      textures: Vec::new(),
      texture_views: Vec::new(),
      render_passes: Vec::new(),
      frame_buffers: Vec::new()
    }
  }

  pub(crate) fn reset(&mut self) {
    self.semaphores.clear();
    self.fences.clear();
    self.buffers.clear();
    self.textures.clear();
    self.texture_views.clear();
    self.render_passes.clear();
    self.frame_buffers.clear();
  }

  pub(crate) fn track_semaphore(&mut self, semaphore: &Arc<Recyclable<VkSemaphore>>) {
    self.semaphores.push(semaphore.clone());
  }

  pub(crate) fn track_fence(&mut self, fence: &Arc<VkFence>) {
    self.fences.push(fence.clone());
  }

  pub(crate) fn track_buffer(&mut self, buffer: &Arc<VkBufferSlice>) {
    self.buffers.push(buffer.clone());
  }

  pub(crate) fn track_texture(&mut self, texture: &Arc<VkTexture>) {
    self.textures.push(texture.clone());
  }

  pub(crate) fn track_render_pass(&mut self, render_pass: &Arc<VkRenderPass>) {
    self.render_passes.push(render_pass.clone());
  }

  pub(crate) fn track_frame_buffer(&mut self, frame_buffer: &Arc<VkFrameBuffer>) {
    self.frame_buffers.push(frame_buffer.clone());
  }

  pub(crate) fn track_texture_view(&mut self, texture_view: &Arc<VkTextureView>) {
    self.texture_views.push(texture_view.clone());
  }
}
