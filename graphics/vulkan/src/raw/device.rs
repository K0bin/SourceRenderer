use std::sync::Arc;
use std::ops::Deref;

use ash::version::{DeviceV1_0};
use ash::vk;

use crate::raw::RawVkInstance;
use VkAdapterExtensionSupport;

pub struct RawVkDevice {
  pub device: ash::Device,
  pub allocator: vk_mem::Allocator,
  pub physical_device: vk::PhysicalDevice,
  pub instance: Arc<RawVkInstance>,
  pub extensions: VkAdapterExtensionSupport
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