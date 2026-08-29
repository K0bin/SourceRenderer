use sourcerenderer_core::platform::Window;
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUInstance, WebGPUSurface};
use std::marker::PhantomData;
use web_sys::OffscreenCanvas;

pub struct WebWindow {
    canvas: OffscreenCanvas,
    fullscreen: bool,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebWindow {
    pub(crate) fn new(canvas: OffscreenCanvas) -> Self {
        Self {
            canvas,
            fullscreen: false,
            _p: PhantomData,
        }
    }
}

impl Window<WebGPUBackend> for WebWindow {
    fn create_surface(&self, _graphics_instance: &WebGPUInstance) -> WebGPUSurface {
        WebGPUSurface::new(self.canvas.clone()).unwrap()
    }

    fn width(&self) -> u32 {
        self.canvas.width()
    }

    fn height(&self) -> u32 {
        self.canvas.height()
    }

    fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }
}
