use sdl3::video::WindowBuilder;
use sourcerenderer_core::platform::GraphicsPlatform;
use sourcerenderer_vulkan::{VkBackend, VkInstance, VkSurface};

use crate::SDLPlatform;

use ash::khr::surface::Instance as SurfaceLoader;
use ash::vk::{
    Handle,
    SurfaceKHR,
};

pub(crate) type SDLGPUBackend = VkBackend;

impl GraphicsPlatform<VkBackend> for SDLPlatform {
    fn create_instance(debug_layers: bool) -> Result<VkInstance, Box<dyn std::error::Error>> {
        Ok(VkInstance::new(debug_layers))
    }
}

pub(crate) fn create_surface(sdl_window_handle: &sdl3::video::Window, graphics_instance: &VkInstance) -> VkSurface {
    let instance_raw = graphics_instance.raw();
    let surface = sdl_window_handle
        .vulkan_create_surface(
            instance_raw.instance.handle().as_raw() as sdl3::video::VkInstance
        )
        .unwrap();
    let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
    VkSurface::new(
        graphics_instance.raw(),
        SurfaceKHR::from_raw(unsafe { std::mem::transmute(surface) }),
        surface_loader,
    )
}

pub(crate) fn prepare_window(window_builder: &mut WindowBuilder) {
    // Loads the Vulkan functions for SDL3. Required for `vulkan_instance_extensions`.
    window_builder.vulkan();
}
