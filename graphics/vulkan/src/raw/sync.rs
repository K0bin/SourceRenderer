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

pub struct RawVkSemaphore {
  pub semaphore: vk::Semaphore,
  pub device: Arc<RawVkDevice>
}

impl RawVkSemaphore {
  pub fn new(device: &Arc<RawVkDevice>, info: &vk::SemaphoreCreateInfo) -> VkResult<Self> {
    let semaphore = unsafe { device.device.create_semaphore(info, None) }?;
    Ok(Self {
      semaphore,
      device: device.clone()
    })
  }
}

impl Deref for RawVkSemaphore {
  type Target = vk::Semaphore;

  fn deref(&self) -> &Self::Target {
    &self.semaphore
  }
}

impl Drop for RawVkSemaphore {
  fn drop(&mut self) {
    unsafe {
      self.device.device.destroy_semaphore(self.semaphore, None)
    }
  }
}

pub struct RawVkFence {
  pub fence: vk::Fence,
  pub device: Arc<RawVkDevice>
}

impl Deref for RawVkFence {
  type Target = vk::Fence;

  fn deref(&self) -> &Self::Target {
    &self.fence
  }
}

impl Drop for RawVkFence {
  fn drop(&mut self) {
    unsafe {
      self.device.device.destroy_fence(self.fence, None)
    }
  }
}