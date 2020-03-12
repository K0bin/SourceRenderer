use std::sync::Arc;
use std::sync::Mutex;

use ash::Device;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::extensions::khr::Surface as SurfaceLoader;
use ash::prelude::VkResult;
use std::ops::Deref;

use crate::raw::RawVkDevice;
use std::marker::PhantomData;

pub struct RawVkCommandPool {
  pub pool: vk::CommandPool,
  pub device: Arc<RawVkDevice>
}

impl RawVkCommandPool {
  pub fn new(device: &Arc<RawVkDevice>, create_info: &vk::CommandPoolCreateInfo) -> VkResult<Self> {
    unsafe {
      device.create_command_pool(create_info, None).map(|pool| Self {
        pool,
        device: device.clone()
      })
    }
  }
}

impl Deref for RawVkCommandPool {
  type Target = vk::CommandPool;

  fn deref(&self) -> &Self::Target {
    &self.pool
  }
}

impl Drop for RawVkCommandPool {
  fn drop(&mut self) {
    unsafe {
      self.device.device.destroy_command_pool(self.pool, None);
    }
  }
}
