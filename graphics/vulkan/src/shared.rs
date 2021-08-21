use ash::vk::Handle;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{RenderPassInfo, Texture};
use sourcerenderer_core::pool::{Pool, Recyclable};
use crate::texture::VkTextureView;
use crate::{VkFenceInner, VkRenderPass, VkSemaphore, VkTexture};
use crate::buffer::BufferAllocator;
use std::sync::{RwLock, Arc};
use crate::descriptor::VkDescriptorSetLayout;
use crate::pipeline::VkPipelineLayout;
use std::collections::HashMap;
use crate::raw::RawVkDevice;
use crate::VkFence;
use crate::sync::{VkEvent, VkFenceState, VkSemaphoreInner};
use crate::renderpass::VkFrameBuffer;

pub struct VkShared {
  device: Arc<RawVkDevice>,
  semaphores: Pool<VkSemaphoreInner>,
  fences: Pool<VkFenceInner>,
  events: Pool<VkEvent>,
  buffers: BufferAllocator, // consider per thread
  descriptor_set_layouts: RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>>,
  pipeline_layouts: RwLock<HashMap<u64, Arc<VkPipelineLayout>>>,
  render_passes: RwLock<HashMap<RenderPassInfo, Arc<VkRenderPass>>>,
  frame_buffers: RwLock<HashMap<SmallVec<[u64; 8]>, Arc<VkFrameBuffer>>>
}

impl VkShared {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let semaphores_device_clone = device.clone();
    let fences_device_clone = device.clone();
    let events_device_clone = device.clone();
    Self {
      device: device.clone(),
      semaphores: Pool::new(Box::new(move ||
        VkSemaphoreInner::new(&semaphores_device_clone)
      )),
      fences: Pool::new(Box::new(move ||
        VkFenceInner::new(&fences_device_clone)
      )),
      events: Pool::new(Box::new(move ||
        VkEvent::new(&events_device_clone)
      )),
      buffers: BufferAllocator::new(device, true),
      descriptor_set_layouts: RwLock::new(HashMap::new()),
      pipeline_layouts: RwLock::new(HashMap::new()),
      render_passes: RwLock::new(HashMap::new()),
      frame_buffers: RwLock::new(HashMap::new())
    }
  }

  #[inline]
  pub(crate) fn get_semaphore(&self) -> Arc<VkSemaphore> {
    Arc::new(VkSemaphore::new(self.semaphores.get()))
  }

  #[inline]
  pub(crate) fn get_event(&self) -> Arc<Recyclable<VkEvent>> {
    let event = self.events.get();
    if event.is_signalled() {
      event.reset();
    }
    Arc::new(event)
  }

  #[inline]
  pub(crate) fn get_fence(&self) -> Arc<VkFence> {
    let inner = self.fences.get();
    let state = inner.state();
    debug_assert_ne!(state, VkFenceState::Submitted);
    if state == VkFenceState::Signalled {
      inner.reset();
    }
    Arc::new(VkFence::new(inner))
  }

  #[inline]
  pub(crate) fn get_descriptor_set_layouts(&self) -> &RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>> {
    &self.descriptor_set_layouts
  }

  #[inline]
  pub(crate) fn get_pipeline_layouts(&self) -> &RwLock<HashMap<u64, Arc<VkPipelineLayout>>> {
    &self.pipeline_layouts
  }

  #[inline]
  pub(crate) fn get_render_passes(&self) -> &RwLock<HashMap<RenderPassInfo, Arc<VkRenderPass>>> {
    &self.render_passes
  }

  pub(crate) fn get_render_pass(&self, info: &RenderPassInfo) -> Arc<VkRenderPass> {
    {
      let cache = self.render_passes.read().unwrap();
      if let Some(renderpass) = cache.get(info) {
        return renderpass.clone();
      }
    }
    let renderpass = Arc::new(VkRenderPass::new(&self.device, info));
    let mut cache = self.render_passes.write().unwrap();
    cache.insert(info.clone(), renderpass.clone());
    renderpass
  }

  pub(crate) fn get_framebuffer(&self, render_pass: &Arc<VkRenderPass>, attachments: &[&Arc<VkTextureView>]) -> Arc<VkFrameBuffer> {
    let key: SmallVec<[u64; 8]> = attachments.iter().map(|a| a.get_view_handle().as_raw()).collect();
    {
      let cache = self.frame_buffers.read().unwrap();
      if let Some(framebuffer) = cache.get(&key) {
        return framebuffer.clone();
      }
    }
    let (width, height) = attachments.iter().fold((0, 0), |old, a| (a.texture().get_info().width.max(old.0), a.texture().get_info().height.max(old.1)));
    let frame_buffer = Arc::new(VkFrameBuffer::new(&self.device, width, height, render_pass, attachments));
    let mut cache = self.frame_buffers.write().unwrap();
    cache.insert(key, frame_buffer.clone());
    frame_buffer
  }

  #[inline]
  pub(crate) fn get_buffer_allocator(&self) -> &BufferAllocator {
    &self.buffers
  }
}
