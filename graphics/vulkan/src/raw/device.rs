use std::sync::Arc;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;

use ash::vk;

use crate::raw::RawVkInstance;
use crate::VkAdapterExtensionSupport;
use crate::queue::VkQueueInfo;

pub struct RawVkDevice {
  pub device: ash::Device,
  pub allocator: vk_mem::Allocator,
  pub physical_device: vk::PhysicalDevice,
  pub instance: Arc<RawVkInstance>,
  pub extensions: VkAdapterExtensionSupport,
  pub graphics_queue_info: VkQueueInfo,
  pub compute_queue_info: Option<VkQueueInfo>,
  pub transfer_queue_info: Option<VkQueueInfo>,
  pub is_alive: AtomicBool
}

impl Deref for RawVkDevice {
  type Target = ash::Device;

  fn deref(&self) -> &Self::Target {
    &self.device
  }
}

impl Drop for RawVkDevice {
  fn drop(&mut self) {
    self.allocator.destroy();
    unsafe {
      self.device.destroy_device(None);
    }
  }
}