use std::sync::Arc;
use std::sync::Mutex;

use ash::Device;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::extensions::khr::Surface as SurfaceLoader;
use ash::prelude::VkResult;

mod device;
mod instance;
mod command;

pub use crate::raw::device::RawVkDevice;
pub use crate::raw::instance::RawVkInstance;
pub use crate::raw::command::RawVkCommandPool;
pub use self::command::RawVkCommandBuffer;

pub struct RawVkImage {
  pub image: vk::Image,
  pub allocation: Option<vk_mem::Allocation>,
  pub device: Arc<RawVkDevice>,
}

impl Drop for RawVkImage {
  fn drop(&mut self) {
    unsafe {
      if let Some(alloc) = &self.allocation {
        self.device.allocator.destroy_image(self.image, alloc).unwrap();
      } else {
        self.device.device.destroy_image(self.image, None)
      }
    }
  }
}

pub struct RawVkImageView {
  pub image_view: vk::ImageView,
  pub image: Arc<vk::Image>,
  pub device: Arc<RawVkDevice>
}

impl Drop for RawVkImageView {
  fn drop(&mut self) {
    unsafe {
      self.device.device.destroy_image_view(self.image_view, None)
    }
  }
}
