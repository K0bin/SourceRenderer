use sourcerenderer_base::graphics::Surface;

use ash::vk::SurfaceKHR;
use ash::extensions::khr::Surface as SurfaceKHRLoader;

use crate::VkBackend;

pub struct VkSurface {
  surface: SurfaceKHR,
  surface_loader: SurfaceKHRLoader
}

impl VkSurface {
  pub fn new(surface: SurfaceKHR, surface_loader: SurfaceKHRLoader) -> Self {
    return VkSurface {
      surface: surface,
      surface_loader: surface_loader
    };
  }

  #[inline]
  pub fn get_surface_handle(&self) -> &SurfaceKHR {
    return &self.surface;
  }

  #[inline]
  pub fn get_surface_loader(&self) -> &SurfaceKHRLoader {
    return &self.surface_loader;
  }
}

impl Surface<VkBackend> for VkSurface {

}
