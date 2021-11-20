use std::sync::Arc;

use web_sys::{Document, HtmlCanvasElement, Window as HTMLWindow, window};

use sourcerenderer_core::platform::Window;
use sourcerenderer_webgl::{WebGLDevice, WebGLInstance, WebGLSurface, WebGLSwapchain};

use crate::platform::WebPlatform;

pub struct WebWindow {
  canvas_selector: String,
  surface: Arc<WebGLSurface>,
  swapchain: Arc<WebGLSwapchain>,
  window: HTMLWindow,
  document: Document
}

impl WebWindow {
  pub(crate) fn new(canvas_selector: &str) -> Self {
    let window = window().unwrap();
    let document = window.document().unwrap();
    let surface = Arc::new(WebGLSurface::new(canvas_selector, &document));
    let swapchain = Arc::new(WebGLSwapchain::new(&surface));
    Self {
      canvas_selector: canvas_selector.to_string(),
      surface,
      swapchain,
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
    self.swapchain.clone()
  }

  fn width(&self) -> u32 {
    1280
  }

  fn height(&self) -> u32 {
    720
  }
}
