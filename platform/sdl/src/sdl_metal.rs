use std::error::Error;

use metal::foreign_types::ForeignTypeRef;
use sdl2::video::WindowBuilder;
use sourcerenderer_core::{platform::Window, Platform};
use sourcerenderer_metal::{MTLBackend, MTLDevice, MTLInstance, MTLSurface, MTLSwapchain};

use raw_window_handle::HasRawWindowHandle;

use crate::{sdl_platform::{SDLWindow, StdIO, StdThreadHandle}, SDLPlatform};

pub(crate) type SDLGPUBackend = MTLBackend;

pub(crate) fn create_instance() -> Result<MTLInstance, Box<dyn Error>> {
    Ok(MTLInstance::new())
}

pub(crate) fn create_surface(sdl_window_handle: &sdl2::video::Window, graphics_instance: &MTLInstance) -> MTLSurface {
    let has_handle: &dyn HasRawWindowHandle = sdl_window_handle;
    let handle = has_handle.raw_window_handle();
    let view = match handle {
        raw_window_handle::RawWindowHandle::UiKit(_) => todo!(),
        raw_window_handle::RawWindowHandle::AppKit(handle) => handle.ns_view,
        _ => unreachable!(),
    };

    let layer = unsafe { sdl2_sys::SDL_Metal_GetLayer(view) };
    let layer_ref = unsafe { metal::MetalLayerRef::from_ptr(std::mem::transmute(layer)) };
    MTLSurface::new(graphics_instance, layer_ref)
}

pub(crate) fn create_swapchain(_vsync: bool, _width: u32, _height: u32, device: &MTLDevice, surface: MTLSurface) -> MTLSwapchain {
    MTLSwapchain::new(surface, device.handle())
}

pub(crate) fn prepare_window(window_builder: &mut WindowBuilder) {
    window_builder.metal_view();
}
