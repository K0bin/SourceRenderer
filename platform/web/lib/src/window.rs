use sourcerenderer_core::platform::Window;
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUInstance, WebGPUSurface};
use std::marker::PhantomData;
use web_sys::{HtmlCanvasElement, OffscreenCanvas};

pub struct WebWindow {
    canvas: HtmlCanvasElement,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebWindow {
    pub(crate) fn new(canvas: HtmlCanvasElement) -> Self {
        Self {
            canvas,
            _p: PhantomData,
        }
    }
}

impl Window<WebGPUBackend> for WebWindow {
    fn create_surface(&self, _graphics_instance: &WebGPUInstance) -> WebGPUSurface {
        WebGPUSurface::new_offscreen(self.canvas.transfer_control_to_offscreen().unwrap()).unwrap()
    }

    fn width(&self) -> u32 {
        self.canvas.width()
    }

    fn height(&self) -> u32 {
        self.canvas.height()
    }
}
