use std::sync::Arc;

use ash::khr::surface::Instance as SurfaceLoader;
use ash::prelude::VkResult;
use ash::vk;
use ash::vk::Handle;
use sourcerenderer_core::gpu;

use super::*;

pub struct VkSurface {
    surface: vk::SurfaceKHR,
    surface_loader: SurfaceLoader,
    instance: Arc<RawVkInstance>,
}

impl VkSurface {
    pub fn new(
        instance: &Arc<RawVkInstance>,
        surface: vk::SurfaceKHR,
        surface_loader: SurfaceLoader,
    ) -> Self {
        Self {
            surface: surface,
            surface_loader,
            instance: instance.clone(),
        }
    }

    #[inline(always)]
    pub fn surface_handle(&self) -> vk::SurfaceKHR {
        self.surface
    }

    #[inline(always)]
    pub fn surface_loader(&self) -> &SurfaceLoader {
        &self.surface_loader
    }

    pub(crate) fn get_capabilities(
        &self,
        physical_device: &vk::PhysicalDevice,
    ) -> VkResult<vk::SurfaceCapabilitiesKHR> {
        let handle = self.surface_handle();
        unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(*physical_device, handle)
        }
    }

    pub(crate) fn get_formats(
        &self,
        physical_device: &vk::PhysicalDevice,
    ) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
        let handle = self.surface_handle();
        unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(*physical_device, handle)
        }
    }

    pub(crate) fn get_present_modes(
        &self,
        physical_device: &vk::PhysicalDevice,
    ) -> VkResult<Vec<vk::PresentModeKHR>> {
        let handle = self.surface_handle();
        unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(*physical_device, handle)
        }
    }
}

impl PartialEq for VkSurface {
    fn eq(&self, other: &Self) -> bool {
        let self_handle = self.surface.as_raw();
        let other_handle = other.surface.as_raw();
        self.instance.instance.handle() == other.instance.instance.handle()
            && self_handle == other_handle
    }
}

impl Eq for VkSurface {}

impl Drop for VkSurface {
    fn drop(&mut self) {
        let handle = self.surface_handle();
        unsafe {
            self.surface_loader.destroy_surface(handle, None);
        }
    }
}

impl gpu::Surface<VkBackend> for VkSurface {
    unsafe fn create_swapchain(
        self,
        width: u32,
        height: u32,
        vsync: bool,
        device: &VkDevice,
    ) -> Result<VkSwapchain, gpu::SwapchainError> {
        VkSwapchain::new(vsync, width, height, device.inner(), self)
    }
}
