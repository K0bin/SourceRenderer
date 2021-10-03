use std::sync::Arc;

use web_sys::HtmlCanvasElement;

use sourcerenderer_core::platform::Window;
use sourcerenderer_webgl::{WebGLDevice, WebGLInstance, WebGLSurface, WebGLSwapchain};

use crate::platform::WebPlatform;

pub struct WebWindow {
  canvas: HtmlCanvasElement
}

impl WebWindow {
  pub(crate) fn new(canvas: &HtmlCanvasElement) -> Self {
    Self {
      canvas: canvas.clone()
    }
  }
}

impl Window<WebPlatform> for WebWindow {
  fn create_surface(&self, _graphics_instance: Arc<WebGLInstance>) -> Arc<WebGLSurface> {
    Arc::new(WebGLSurface::new(&self.canvas))
  }

  fn create_swapchain(&self, _vsync: bool, _device: &WebGLDevice, surface: &Arc<WebGLSurface>) -> Arc<WebGLSwapchain> {
    Arc::new(WebGLSwapchain::new(surface))
  }
}
