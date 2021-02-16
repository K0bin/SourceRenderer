use sourcerenderer_core::graphics::Surface;

use ash::vk;
use ash::extensions::khr::Surface as SurfaceLoader;

use crate::raw::*;
use std::sync::Arc;

use ash::prelude::VkResult;
use ash::version::InstanceV1_0;
use std::sync::atomic::AtomicBool;

pub struct VkSurface {
  surface: vk::SurfaceKHR,
  surface_loader: SurfaceLoader,
  instance: Arc<RawVkInstance>,
  is_lost: AtomicBool
}

impl VkSurface {
  pub fn new(instance: &Arc<RawVkInstance>, surface: vk::SurfaceKHR, surface_loader: SurfaceLoader) -> Self {
    return VkSurface {
      surface,
      surface_loader,
      instance: instance.clone(),
      is_lost: AtomicBool::new(false)
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

  pub(crate) fn get_capabilities(&self, physical_device: &vk::PhysicalDevice) -> VkResult<vk::SurfaceCapabilitiesKHR> {
    unsafe {
      self.surface_loader.get_physical_device_surface_capabilities(*physical_device, self.surface)
    }
  }

  pub(crate) fn get_formats(&self, physical_device: &vk::PhysicalDevice) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
    unsafe {
      self.surface_loader.get_physical_device_surface_formats(*physical_device, self.surface)
    }
  }

  pub(crate) fn get_present_modes(&self, physical_device: &vk::PhysicalDevice) -> VkResult<Vec<vk::PresentModeKHR>> {
    unsafe {
      self.surface_loader.get_physical_device_surface_present_modes(*physical_device, self.surface)
    }
  }
}

impl PartialEq for VkSurface {
  fn eq(&self, other: &Self) -> bool {
    self.instance.instance.handle() == other.instance.instance.handle() && self.surface == other.surface
  }
}

impl Eq for VkSurface {}

impl Drop for VkSurface {
  fn drop(&mut self) {
    unsafe {
      self.surface_loader.destroy_surface(self.surface, None);
    }
  }
}

impl Surface for VkSurface {

}
