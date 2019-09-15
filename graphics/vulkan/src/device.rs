use std::sync::Arc;
use std::sync::Weak;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::{ Adapter, Device, AdapterType, Queue, QueueType };
use crate::queue::VkQueue;
use crate::queue::VkQueueInfo;
use crate::adapter::VkAdapter;

pub struct VkDevice {
  adapter: Arc<VkAdapter>,
  device: ash::Device,
  graphics_queue_info: VkQueueInfo,
  compute_queue_info: Option<VkQueueInfo>,
  transfer_queue_info: Option<VkQueueInfo>
}

impl VkDevice {
  pub fn new(
    adapter: Arc<VkAdapter>,
    device: ash::Device,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>) -> Self {


    return VkDevice {
      adapter: adapter,
      device: device,
      graphics_queue_info: graphics_queue_info,
      compute_queue_info: compute_queue_info,
      transfer_queue_info: transfer_queue_info
    };
  }

  pub fn get_device(&self) -> &ash::Device {
    return &self.device;
  }

  pub fn get_adapter(&self) -> &VkAdapter {
    return self.adapter.as_ref();
  }
}

impl Drop for VkDevice {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_device(None);
    }
  }
}

impl Device for VkDevice {
  fn create_queue(self: Arc<Self>, queue_type: QueueType) -> Option<Arc<dyn Queue>> {
    return match queue_type {
      QueueType::GRAPHICS => {
        let vk_queue = unsafe { self.device.get_device_queue(self.graphics_queue_info.queue_family_index as u32, self.graphics_queue_info.queue_index as u32) };
        return Some(Arc::new(VkQueue::new(self.graphics_queue_info.clone(), vk_queue, self.clone())));
      }
      QueueType::COMPUTE => {
        self.compute_queue_info.map(|info| {
            let vk_queue = unsafe { self.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
            Arc::new(VkQueue::new(info.clone(), vk_queue, self.clone())) as Arc<dyn Queue>
          }
        )
      }
      QueueType::TRANSFER => {
        self.transfer_queue_info.map(|info| {
            let vk_queue = unsafe { self.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
            Arc::new(VkQueue::new(info.clone(), vk_queue, self.clone())) as Arc<dyn Queue>
          }
        )
      }
    }
  }
}