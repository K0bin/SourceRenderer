use std::sync::Arc;
use std::ops::Deref;

use ash::version::{DeviceV1_0};
use ash::vk;
use ash::prelude::VkResult;

use crate::raw::RawVkDevice;

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
