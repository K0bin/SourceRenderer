use sourcerenderer_core::graphics::Surface;

use ash::vk;
use ash::extensions::khr::Surface as SurfaceLoader;

use crate::VkBackend;
use crate::raw::*;
use std::sync::Arc;
use std::cmp::{min, max};

pub struct VkSurface {
  surface: vk::SurfaceKHR,
  surface_loader: SurfaceLoader,
  instance: Arc<RawVkInstance>
}

impl VkSurface {
  pub fn new(instance: &Arc<RawVkInstance>, surface: vk::SurfaceKHR, surface_loader: SurfaceLoader) -> Self {
    return VkSurface {
      surface,
      surface_loader,
      instance: instance.clone()
    };
  }

  #[inline]
  pub fn get_surface_handle(&self) -> &vk::SurfaceKHR {
    return &self.surface;
  }

  #[inline]
  pub fn get_surface_loader(&self) -> &SurfaceLoader {
    return &self.surface_loader;
  }

  pub fn get_extent(&self, device: &Arc<RawVkDevice>, preferred_width: u32, preferred_height: u32) -> (u32, u32) {
    let capabilities = unsafe {
      self.surface_loader.get_physical_device_surface_capabilities(device.physical_device, self.surface)
    }.unwrap();

    if capabilities.current_extent.width != u32::MAX && capabilities.current_extent.height != u32::MAX {
      (capabilities.current_extent.width, capabilities.current_extent.height)
    } else {
      (
        min(max(preferred_width, capabilities.min_image_extent.width), capabilities.max_image_extent.width),
        min(max(preferred_height, capabilities.min_image_extent.height), capabilities.max_image_extent.height)
      )
    }
  }
}

impl Drop for VkSurface {
  fn drop(&mut self) {
    unsafe {
      self.surface_loader.destroy_surface(self.surface, None);
    }
  }
}

impl Surface for VkSurface {

}
