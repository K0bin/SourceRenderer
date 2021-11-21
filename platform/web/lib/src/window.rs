use std::sync::Arc;

use web_sys::{Document, HtmlCanvasElement, Window as HTMLWindow, window};

use sourcerenderer_core::platform::Window;
use sourcerenderer_webgl::{WebGLDevice, WebGLInstance, WebGLSurface, WebGLSwapchain};

use crate::platform::WebPlatform;

pub struct WebWindow {
  surface: Arc<WebGLSurface>,
  window: HTMLWindow,
  document: Document,
  canvas: HtmlCanvasElement
}

impl WebWindow {
  pub(crate) fn new(canvas: HtmlCanvasElement) -> Self {
    let id = canvas.id();
    if id.is_empty() {
      panic!("Canvas needs a unique id.");
    }

    let window = window().unwrap();
    let document = window.document().unwrap();
    let surface = Arc::new(WebGLSurface::new(&format!("#{}", id), &document));
    Self {
      canvas,
      surface,
      window,
      document
    }
  }

  pub fn document(&self) -> &Document {
    &self.document
  }

  pub fn window(&self) -> &HTMLWindow {
    &self.window
  }
}

impl Window<WebPlatform> for WebWindow {
  fn create_surface(&self, _graphics_instance: Arc<WebGLInstance>) -> Arc<WebGLSurface> {
    self.surface.clone()
  }

  fn create_swapchain(&self, _vsync: bool, device: &WebGLDevice, surface: &Arc<WebGLSurface>) -> Arc<WebGLSwapchain> {
    Arc::new(WebGLSwapchain::new(&surface, device.sender(), device.handle_allocator()))
  }

  fn width(&self) -> u32 {
    self.canvas.width()
  }

  fn height(&self) -> u32 {
    self.canvas.height()
  }
}
