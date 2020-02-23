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

pub struct RawVkSurface {
  pub surface: vk::SurfaceKHR,
  pub surface_loader: SurfaceLoader,
  pub instance: Arc<RawVkInstance>
}

impl Deref for RawVkSurface {
  type Target = vk::SurfaceKHR;

  fn deref(&self) -> &Self::Target {
    &self.surface
  }
}

impl Drop for RawVkSurface {
  fn drop(&mut self) {
    unsafe {
      self.surface_loader.destroy_surface(self.surface, None);
    }
  }
}
