use std::sync::Arc;

use ash::vk;

mod device;
mod instance;
mod command;

pub use crate::raw::device::RawVkDevice;
pub use crate::raw::instance::RawVkInstance;
pub use crate::raw::instance::RawVkDebugUtils;
pub use crate::raw::command::RawVkCommandPool;
pub use crate::raw::device::VkFeatures;

pub struct RawVkImage {
  pub image: vk::Image,
  pub allocation: Option<vk_mem::Allocation>,
  pub device: Arc<RawVkDevice>,
}

impl Drop for RawVkImage {
  fn drop(&mut self) {
    unsafe {
      if let Some(alloc) = self.allocation {
        self.device.allocator.destroy_image(self.image, alloc);
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
