use std::sync::{Arc, RwLock};
use std::sync::Mutex;

use ash::Device;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::extensions::khr::Surface as SurfaceLoader;
use ash::prelude::VkResult;
use std::ops::Deref;

use crate::raw::RawVkInstance;

pub struct RawVkDevice {
  pub device: ash::Device,
  pub allocator: vk_mem::Allocator,
  pub physical_device: vk::PhysicalDevice,
  pub instance: Arc<RawVkInstance>
}

impl RawVkDevice {
  pub fn new(instance: &Arc<RawVkInstance>, physical_device: vk::PhysicalDevice, create_info: &vk::DeviceCreateInfo, allocator_create_info: &vk_mem::AllocatorCreateInfo) -> VkResult<Self> {
    unsafe {
      let device = instance.create_device(physical_device, create_info, None)?;
      let allocator = vk_mem::Allocator::new(allocator_create_info).unwrap();
      Ok(Self {
        device,
        allocator,
        physical_device,
        instance: instance.clone()
      })
    }
  }
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