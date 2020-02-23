use std::sync::Arc;
use std::sync::Mutex;

use ash::Device;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::extensions::khr::Surface as SurfaceLoader;
use ash::prelude::VkResult;
use std::ops::Deref;

pub struct RawVkInstance {
  pub entry: ash::Entry,
  pub instance: ash::Instance
}

impl Deref for RawVkInstance {
  type Target = ash::Instance;

  fn deref(&self) -> &Self::Target {
    &self.instance
  }
}

impl Drop for RawVkInstance {
  fn drop(&mut self) {
    unsafe {
      self.instance.destroy_instance(None);
    }
  }
}
