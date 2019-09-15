use std::sync::Arc;
use std::sync::Mutex;
use std::rc::Rc;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Adapter;
use sourcerenderer_core::graphics::Device;
use sourcerenderer_core::graphics::Queue;
use sourcerenderer_core::graphics::QueueType;
use sourcerenderer_core::graphics::CommandPool;
use crate::device::VkDevice;
use crate::command::VkCommandPool;

#[derive(Clone, Debug, Copy)]
pub struct VkQueueInfo {
  pub queue_family_index: usize,
  pub queue_index: usize,
  pub queue_type: QueueType,
  pub supports_presentation: bool
}

pub struct VkQueue {
  info: VkQueueInfo,
  queue: Mutex<vk::Queue>,
  device: Arc<VkDevice>
}

impl VkQueue {
  pub fn new(info: VkQueueInfo, queue: vk::Queue, device: Arc<VkDevice>) -> Self {
    return VkQueue {
      info: info,
      queue: Mutex::new(queue),
      device: device
    };
  }

  pub fn get_queue_family_index(&self) -> u32 {
    return self.info.queue_family_index as u32;
  }

  pub fn get_device(&self) -> &VkDevice {
    return self.device.as_ref();
  }
}

// Vulkan queues are implicitly freed with the logical device

impl Queue for VkQueue {
  fn create_command_pool(self: Arc<Self>) -> Rc<CommandPool> {
    return Rc::new(VkCommandPool::new(self.clone()));
  }

  fn get_queue_type(&self) -> QueueType {
    return self.info.queue_type;
  }

  fn supports_presentation(&self) -> bool {
    return self.info.supports_presentation;
  }
}
