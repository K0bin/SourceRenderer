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
use crate::raw::RawVkInstance;

pub struct RawVkSwapchain {
  pub swap_chain: vk::SwapchainKHR,
  pub swap_chain_loader: SwapchainLoader,
  pub instance: Arc<RawVkInstance>
}

impl Deref for RawVkSwapchain {
  type Target = vk::SwapchainKHR;

  fn deref(&self) -> &Self::Target {
    &self.swap_chain
  }
}

impl Drop for RawVkSwapchain {
  fn drop(&mut self) {
    unsafe {
      self.swap_chain_loader.destroy_swapchain(self.swap_chain, None)
    }
  }
}