use sourcerenderer_core::graphics::Surface;

use ash::vk;
use ash::extensions::khr::Surface as SurfaceLoader;

use crate::VkBackend;
use crate::raw::*;
use std::sync::Arc;

pub struct VkSurface {
  pub surface: vk::SurfaceKHR,
  pub surface_loader: SurfaceLoader,
  pub instance: Arc<RawVkInstance>
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
