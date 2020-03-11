use std::collections::HashMap;
use std::thread::ThreadId;
use std::sync::{Arc, Mutex};
use std::sync::RwLock;

use thread_local::ThreadLocal;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkCommandPool;
use sourcerenderer_core::graphics::Device;
use std::cell::{RefCell, RefMut};
use VkPipeline;

pub struct VkSharedCaches {
  pub pipelines: RwLock<HashMap<u64, VkPipeline>>

}

pub struct VkGraphicsContext {
  device: Arc<RawVkDevice>,
  threads: ThreadLocal<VkThreadContext>,
  caches: Arc<VkSharedCaches>
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadContext {
  device: Arc<RawVkDevice>,
  frames: Vec<RefCell<VkFrameContext>>
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameContext {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool
}

impl VkGraphicsContext {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    return VkGraphicsContext {
      device: device.clone(),
      threads: ThreadLocal::new(),
      caches: Arc::new(VkSharedCaches::new())
    };
  }

  pub fn get_caches(&self) -> &Arc<VkSharedCaches> {
    &self.caches
  }

  pub fn get_thread_context(&self) -> &VkThreadContext {
    &self.threads.get_or(|| VkThreadContext::new(&self.device))
  }
}

impl VkThreadContext {
  fn new(device: &Arc<RawVkDevice>) -> Self {
    return VkThreadContext {
      device: device.clone(),
      frames: Vec::new()
    };
  }

  pub fn get_frame_context(&self, index: u64) -> RefMut<VkFrameContext> {
    self.frames[(index as usize) % self.frames.len()].borrow_mut()
  }
}

impl VkFrameContext {
  pub fn get_command_pool(&mut self) -> &mut VkCommandPool {
    &mut self.command_pool
  }
}

impl VkSharedCaches {
  pub fn new() -> Self {
    Self {
      pipelines: RwLock::new(HashMap::new())
    }
  }

  pub fn get_pipelines(&self) -> &RwLock<HashMap<u64, VkPipeline>> {
    &self.pipelines
  }
}

