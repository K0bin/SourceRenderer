use std::error::Error;

use sdl2::video::WindowBuilder;
use sourcerenderer_vulkan::{VkBackend, VkDevice, VkInstance, VkSurface, VkSwapchain};

use crate::sdl_platform::SDLWindow;

use ash::khr::surface::Instance as SurfaceLoader;
use ash::vk::{
    Handle,
    SurfaceKHR,
};

pub(crate) type SDLGPUBackend = VkBackend;

pub(crate) fn create_instance(debug_layers: bool, window: &SDLWindow) -> Result<VkInstance, Box<dyn Error>> {
    let instance_extensions = window.sdl_window_handle().vulkan_instance_extensions()?;
    Ok(VkInstance::new(&instance_extensions, debug_layers))
}

pub(crate) fn create_surface(sdl_window_handle: &sdl2::video::Window, graphics_instance: &VkInstance) -> VkSurface {
    let instance_raw = graphics_instance.raw();
    let surface = sdl_window_handle
        .vulkan_create_surface(
            instance_raw.instance.handle().as_raw() as sdl2::video::VkInstance
        )
        .unwrap();
    let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
    VkSurface::new(
        graphics_instance.raw(),
        SurfaceKHR::from_raw(surface),
        surface_loader,
    )
}

pub(crate) fn create_swapchain(vsync: bool, width: u32, height: u32, device: &VkDevice, surface: VkSurface) -> VkSwapchain {
    let device_inner = device.inner();
    VkSwapchain::new(
        vsync,
        width,
        height,
        device_inner,
        surface
    )
    .unwrap()
}

pub(crate) fn prepare_window(window_builder: &mut WindowBuilder) {
    window_builder.vulkan();
}
