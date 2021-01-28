use sourcerenderer_core::pool::{Pool, Recyclable};
use crate::{VkSemaphore, VkFenceInner};
use crate::buffer::BufferAllocator;
use std::sync::{RwLock, Arc};
use crate::descriptor::VkDescriptorSetLayout;
use crate::pipeline::VkPipelineLayout;
use std::collections::HashMap;
use crate::raw::RawVkDevice;
use crate::VkFence;

pub struct VkShared {
  semaphores: Pool<VkSemaphore>,
  fences: Pool<VkFenceInner>,
  buffers: BufferAllocator, // consider per thread
  descriptor_set_layouts: RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>>,
  pipeline_layouts: RwLock<HashMap<u64, Arc<VkPipelineLayout>>>
}

impl VkShared {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let semaphores_device_clone = device.clone();
    let fences_device_clone = device.clone();
    Self {
      semaphores: Pool::new(Box::new(move ||
        VkSemaphore::new(&semaphores_device_clone)
      )),
      fences: Pool::new(Box::new(move ||
        VkFenceInner::new(&fences_device_clone)
      )),
      buffers: BufferAllocator::new(device),
      descriptor_set_layouts: RwLock::new(HashMap::new()),
      pipeline_layouts: RwLock::new(HashMap::new())
    }
  }

  #[inline]
  pub(crate) fn get_semaphore(&self) -> Arc<Recyclable<VkSemaphore>> {
    Arc::new(self.semaphores.get())
  }

  #[inline]
  pub(crate) fn get_fence(&self) -> Arc<VkFence> {
    let inner = self.fences.get();
    if inner.is_signalled() {
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
  pub(crate) fn get_buffer_allocator(&self) -> &BufferAllocator {
    &self.buffers
  }
}
