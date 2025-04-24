use sourcerenderer_core::platform::Window;
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUInstance, WebGPUSurface};
use web_sys::OffscreenCanvas;

enum CanvasKind {
    Canvas(OffscreenCanvas),
    Fake { width: u32, height: u32 },
}

pub struct WebWindow {
    canvas: CanvasKind,
}

impl WebWindow {
    pub(crate) fn new(canvas: OffscreenCanvas) -> Self {
        Self {
            canvas: CanvasKind::Canvas(canvas),
        }
    }
    pub(crate) fn new_fake(width: u32, height: u32) -> Self {
        Self {
            canvas: CanvasKind::Fake { width, height },
        }
    }
}

impl Window<WebGPUBackend> for WebWindow {
    fn create_surface(&self, graphics_instance: &WebGPUInstance) -> WebGPUSurface {
        match &self.canvas {
            CanvasKind::Canvas(canvas) => {
                WebGPUSurface::new(graphics_instance, canvas.clone()).unwrap()
            }
            CanvasKind::Fake { .. } => WebGPUSurface::new_fake(graphics_instance).unwrap(),
        }
    }

    fn width(&self) -> u32 {
        match &self.canvas {
            CanvasKind::Canvas(canvas) => canvas.width(),
            CanvasKind::Fake { width, .. } => *width,
        }
    }

    fn height(&self) -> u32 {
        match &self.canvas {
            CanvasKind::Canvas(canvas) => canvas.height(),
            CanvasKind::Fake { height, .. } => *height,
        }
    }
}
