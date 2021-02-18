use sourcerenderer_core::graphics::Surface;

use ash::vk;
use ash::extensions::khr::Surface as SurfaceLoader;

use crate::raw::*;
use std::sync::{Arc, Mutex, MutexGuard};

use ash::prelude::VkResult;
use ash::version::InstanceV1_0;
use std::sync::atomic::{AtomicBool, Ordering};
use ash::vk::Handle;

pub struct VkSurface {
  surface: Mutex<vk::SurfaceKHR>,
  surface_loader: SurfaceLoader,
  instance: Arc<RawVkInstance>,
  is_lost: AtomicBool
}

impl VkSurface {
  pub fn new(instance: &Arc<RawVkInstance>, surface: vk::SurfaceKHR, surface_loader: SurfaceLoader) -> Self {
    Self {
      surface: Mutex::new(surface),
      surface_loader,
      instance: instance.clone(),
      is_lost: AtomicBool::new(false)
    }
  }

  #[inline]
  pub fn get_surface_handle(&self) -> MutexGuard<vk::SurfaceKHR> {
    self.surface.lock().unwrap()
  }

  #[inline]
  pub fn get_surface_loader(&self) -> &SurfaceLoader {
    &self.surface_loader
  }

  pub(crate) fn get_capabilities(&self, physical_device: &vk::PhysicalDevice) -> VkResult<vk::SurfaceCapabilitiesKHR> {
    let handle = self.get_surface_handle();
    unsafe {
      self.surface_loader.get_physical_device_surface_capabilities(*physical_device, *handle)
    }
  }

  pub(crate) fn get_formats(&self, physical_device: &vk::PhysicalDevice) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
    let handle = self.get_surface_handle();
    unsafe {
      self.surface_loader.get_physical_device_surface_formats(*physical_device, *handle)
    }
  }

  pub(crate) fn get_present_modes(&self, physical_device: &vk::PhysicalDevice) -> VkResult<Vec<vk::PresentModeKHR>> {
    let handle = self.get_surface_handle();
    unsafe {
      self.surface_loader.get_physical_device_surface_present_modes(*physical_device, *handle)
    }
  }

  pub fn is_lost(&self) -> bool {
    self.is_lost.load(Ordering::SeqCst)
  }

  pub fn mark_lost(&self) {
    self.is_lost.store(true, Ordering::SeqCst);
  }
}

impl PartialEq for VkSurface {
  fn eq(&self, other: &Self) -> bool {
    let self_handle = self.surface.lock().unwrap().as_raw();
    let other_handle = other.surface.lock().unwrap().as_raw();
    self.instance.instance.handle() == other.instance.instance.handle() && self_handle == other_handle
  }
}

impl Eq for VkSurface {}

impl Drop for VkSurface {
  fn drop(&mut self) {
    let handle = self.get_surface_handle();
    unsafe {
      self.surface_loader.destroy_surface(*handle, None);
    }
  }
}

impl Surface for VkSurface {

}
