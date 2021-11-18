use std::sync::Arc;

use web_sys::{Document, HtmlCanvasElement, Window as HTMLWindow, window};

use sourcerenderer_core::platform::Window;
use sourcerenderer_webgl::{WebGLDevice, WebGLInstance, WebGLSurface, WebGLSwapchain};

use crate::platform::WebPlatform;

pub struct WebWindow {
  canvas_selector: String,
  surface: Arc<WebGLSurface>,
  window: HTMLWindow,
  document: Document
}

impl WebWindow {
  pub(crate) fn new(canvas_selector: &str) -> Self {
    let window = window().unwrap();
    let document = window.document().unwrap();
    let surface = Arc::new(WebGLSurface::new(canvas_selector, &document));
    Self {
      canvas_selector: canvas_selector.to_string(),
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
    let swapchain = Arc::new(WebGLSwapchain::new(device.sender(), &surface));
    swapchain
  }

  fn width(&self) -> u32 {
    1280
  }

  fn height(&self) -> u32 {
    720
  }
}
