use std::{error::Error, ffi::c_void};

use objc2::rc::Retained;
use sdl3::video::WindowBuilder;
use sourcerenderer_core::platform::GraphicsPlatform;
use sourcerenderer_metal::{MTLBackend, MTLInstance, MTLSurface};

use raw_window_handle::HasWindowHandle;

use crate::SDLPlatform;

pub(crate) type SDLGPUBackend = MTLBackend;

impl GraphicsPlatform<MTLBackend> for SDLPlatform {
    fn create_instance(debug_layers: bool) -> Result<MTLInstance, Box<dyn Error>> {
        Ok(MTLInstance::new(debug_layers))
    }
}

pub(crate) fn create_surface(sdl_window_handle: &sdl3::video::Window, graphics_instance: &MTLInstance) -> MTLSurface {
    let has_handle: &dyn HasWindowHandle = sdl_window_handle;
    let handle = has_handle.window_handle();
    let view = match handle.expect("Failed to get window handle").as_raw() {
        raw_window_handle::RawWindowHandle::UiKit(_) => todo!(),
        raw_window_handle::RawWindowHandle::AppKit(handle) => handle.ns_view,
        _ => unreachable!(),
    };

    let mut layer: *mut c_void = std::ptr::null_mut();
    let ns_view = view.as_ptr() as *mut objc2_app_kit::NSView;
    unsafe {
        for subview in (*ns_view).subviews().iter() {
            let subview_ptr = Retained::into_raw(subview);
            layer = sdl3_sys::everything::SDL_Metal_GetLayer(subview_ptr as *mut c_void);
            let _ = Retained::from_raw(subview_ptr);
            if !layer.is_null() {
                break;
            }
        }
    }

    let layer_ref: Retained<objc2_quartz_core::CAMetalLayer> = unsafe { Retained::from_raw(layer as *mut objc2_quartz_core::CAMetalLayer).unwrap() };
    std::mem::forget(layer_ref.clone()); // Increase ref count, Retained::from_raw doesn't do that.
    MTLSurface::new(graphics_instance, layer_ref)
}

pub(crate) fn prepare_window(window_builder: &mut WindowBuilder) {
    window_builder.metal_view();
}
