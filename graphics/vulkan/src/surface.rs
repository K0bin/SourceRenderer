use sourcerenderer_core::graphics::Surface;

use ash::vk::SurfaceKHR;
use ash::extensions::khr::Surface as SurfaceKHRLoader;

use crate::VkBackend;
use crate::raw::*;
use std::sync::Arc;

pub struct VkSurface {
  surface: Arc<RawVkSurface>
}

impl VkSurface {
  pub fn new(instance: &Arc<RawVkInstance>, surface: SurfaceKHR, surface_loader: SurfaceKHRLoader) -> Self {
    return VkSurface {
      surface: Arc::new(RawVkSurface {
        surface,
        surface_loader,
        instance: instance.clone()
      })
    };
  }

  #[inline]
  pub fn get_surface_handle(&self) -> &SurfaceKHR {
    return &self.surface.surface;
  }

  #[inline]
  pub fn get_surface_loader(&self) -> &SurfaceKHRLoader {
    return &self.surface.surface_loader;
  }

  pub fn get_raw(&self) -> &Arc<RawVkSurface> {
    return &self.surface;
  }
}

impl Surface<VkBackend> for VkSurface {

}
