use std::sync::Arc;
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
      //let guard = self.instance.alloc_callbacks.map(|ref callbacks| callbacks.lock().unwrap());
      //self.device.destroy_device(guard.map(|callbacks| &*callbacks));
      self.device.destroy_device(None);
    }
  }
}