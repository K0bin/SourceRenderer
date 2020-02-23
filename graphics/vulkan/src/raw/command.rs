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

pub struct RawVkCommandPool {
  pub pool: vk::CommandPool,
  pub device: Arc<RawVkDevice>
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

pub struct RawVkCommandBuffer {
  pub buffer: vk::CommandBuffer,
  pub device: Arc<RawVkDevice>,
  pub pool: Arc<RawVkCommandPool>
}

impl Deref for RawVkCommandBuffer {
  type Target = vk::CommandBuffer;

  fn deref(&self) -> &Self::Target {
    &self.buffer
  }
}

impl Drop for RawVkCommandBuffer {
  fn drop(&mut self) {
    unsafe {
      self.device.device.free_command_buffers(self.pool.pool, &[ self.buffer ])
    }
  }
}