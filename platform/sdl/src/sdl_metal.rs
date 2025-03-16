use std::error::Error;

use objc2::rc::Retained;
use sdl2::video::WindowBuilder;
use sourcerenderer_core::platform::GraphicsPlatform;
use sourcerenderer_metal::{MTLBackend, MTLDevice, MTLInstance, MTLSurface, MTLSwapchain};

use raw_window_handle::HasWindowHandle;

use crate::SDLPlatform;

pub(crate) type SDLGPUBackend = MTLBackend;

impl GraphicsPlatform<MTLBackend> for SDLPlatform {
    fn create_instance(&self, debug_layers: bool) -> Result<MTLInstance, Box<dyn Error>> {
        Ok(MTLInstance::new(debug_layers))
    }
}

pub(crate) fn create_surface(sdl_window_handle: &sdl2::video::Window, graphics_instance: &MTLInstance) -> MTLSurface {
    let has_handle: &dyn HasWindowHandle = sdl_window_handle;
    let handle = has_handle.window_handle();
    let view = match handle.expect("Failed to get window handle").as_raw() {
        raw_window_handle::RawWindowHandle::UiKit(_) => todo!(),
        raw_window_handle::RawWindowHandle::AppKit(handle) => handle.ns_view,
        _ => unreachable!(),
    };

    let layer = unsafe { sdl2_sys::SDL_Metal_GetLayer(view.as_ptr()) };
    let layer_ref: Retained<objc2_quartz_core::CAMetalLayer> = unsafe { Retained::from_raw(std::mem::transmute(layer)).unwrap() };
    std::mem::forget(layer_ref.clone()); // Increase ref count, Retained::from_raw doesn't do that.
    MTLSurface::new(graphics_instance, layer_ref)
}

pub(crate) fn create_swapchain(_vsync: bool, width: u32, height: u32, device: &MTLDevice, surface: MTLSurface) -> MTLSwapchain {
    unsafe {
        MTLSwapchain::new(surface, device.handle(), Some((width, height)))
    }
}

pub(crate) fn prepare_window(window_builder: &mut WindowBuilder) {
    window_builder.metal_view();
}
