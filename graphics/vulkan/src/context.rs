use std::collections::HashMap;
use std::thread::ThreadId;
use std::sync::{Arc, Mutex};
use std::sync::RwLock;

use thread_local::ThreadLocal;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkCommandPool;
use sourcerenderer_core::graphics::Device;

pub struct VkGraphicsContext {
  device: Arc<RawVkDevice>,
  threads: ThreadLocal<VkThreadContext>
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadContext {
  device: Arc<RawVkDevice>,
  frames: Vec<VkFrameContext>
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameContext {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool
}

impl VkGraphicsContext {
  fn new(device: &Arc<RawVkDevice>) -> Self {
    return VkGraphicsContext {
      device: device.clone(),
      threads: ThreadLocal::new()
    };
  }

  /*fn get_thread_context(&self) -> &VkThreadContext {
    self.threads.get_or(|| VkThreadContext::new(&self.device))
  }*/
}

impl VkThreadContext {
  fn new(device: &Arc<RawVkDevice>) -> Self {
    return VkThreadContext {
      device: device.clone(),
      frames: Vec::new()
    };
  }
}
