use std::sync::Arc;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Adapter;
use sourcerenderer_core::graphics::Device;
use sourcerenderer_core::graphics::AdapterType;
use sourcerenderer_core::graphics::Queue;
use crate::device::VkDevice;

#[derive(Clone, Debug)]
pub struct VkQueueInfo {
  pub queue_family_index: usize,
  pub queue_index: usize
}

pub struct VkQueue {
  info: VkQueueInfo,
  queue: vk::Queue
}

impl VkQueue {
  pub fn new(info: VkQueueInfo, queue: vk::Queue) -> Self {
    return VkQueue {
      info: info,
      queue: queue
    };
  }
}

// Vulkan queues are implicitly freed with the logical device

impl Queue for VkQueue {

}
