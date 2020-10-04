use std::sync::Arc;
use std::sync::Mutex;

use ash::{Device, InstanceError};
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::extensions::khr::Surface as SurfaceLoader;
use ash::prelude::VkResult;
use std::ops::Deref;

pub struct RawVkInstance {
  pub entry: ash::Entry,
  pub instance: ash::Instance,
  pub debug_utils_loader: ash::extensions::ext::DebugUtils,
  pub debug_messenger: vk::DebugUtilsMessengerEXT
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
      self.debug_utils_loader.destroy_debug_utils_messenger(self.debug_messenger, None);
      self.instance.destroy_instance(None);
    }
  }
}
