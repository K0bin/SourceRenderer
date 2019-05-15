use sourcerenderer_core::graphics::Surface;

use ash::vk::SurfaceKHR;

pub struct VkSurface {
  surface: SurfaceKHR
}

impl VkSurface {
  pub fn new(surface: SurfaceKHR) -> Self{
    return VkSurface {
      surface: surface
    };
  }

  #[inline]
  pub fn surface(&self) -> SurfaceKHR {
    return self.surface;
  }
}

impl Surface for VkSurface {

}
