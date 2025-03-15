use sourcerenderer_core::platform::Window;
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUDevice, WebGPUInstance, WebGPUSurface, WebGPUSwapchain};
use web_sys::OffscreenCanvas;

pub struct WebWindow {
    canvas: OffscreenCanvas
}

impl WebWindow {
    pub(crate) fn new(canvas: OffscreenCanvas) -> Self {
        Self {
            canvas
        }
    }
}

impl Window<WebGPUBackend> for WebWindow {
    fn create_surface(&self, graphics_instance: &WebGPUInstance) -> WebGPUSurface {
        WebGPUSurface::new(graphics_instance, self.canvas.clone()).unwrap()
    }

    fn create_swapchain(&self, _vsync: bool, device: &WebGPUDevice, surface: WebGPUSurface) -> WebGPUSwapchain {
        WebGPUSwapchain::new(device.handle(), surface)
    }

    fn width(&self) -> u32 {
        self.canvas.width()
    }

    fn height(&self) -> u32 {
        self.canvas.height()
    }
}
