use std::sync::Arc;

use sourcerenderer_core::graphics::{Format, SampleCount, Surface, Swapchain};
use web_sys::HtmlCanvasElement;

use crate::{WebGLBackend, sync::WebGLSemaphore, texture::WebGLRenderTargetView};

pub struct WebGLSurface {
  canvas_element: HtmlCanvasElement
}

unsafe impl Send for WebGLSurface {}
unsafe impl Sync for WebGLSurface {}

impl Surface for WebGLSurface {}

impl PartialEq for WebGLSurface {
  fn eq(&self, other: &Self) -> bool {
    self.canvas_element == other.canvas_element
  }
}

impl Eq for WebGLSurface {}

impl WebGLSurface {
  pub fn new(canvas_element: &HtmlCanvasElement) -> Self {
    Self {
      canvas_element: canvas_element.clone()
    }
  }

  pub fn canvas(&self) -> &HtmlCanvasElement {
    &self.canvas_element
  }
}

pub struct WebGLSwapchain {
  surface: Arc<WebGLSurface>,
  width: u32,
  height: u32
}

impl WebGLSwapchain {
  pub fn new(surface: &Arc<WebGLSurface>) -> Self {
    
    Self {
      surface: surface.clone(),
      width: surface.canvas().width(),
      height: surface.canvas().height()
    }
  }
}

impl Swapchain<WebGLBackend> for WebGLSwapchain {
  fn recreate(old: &Self, _width: u32, _height: u32) -> Result<std::sync::Arc<Self>, sourcerenderer_core::graphics::SwapchainError> {
    Ok(
      Arc::new(WebGLSwapchain::new(&old.surface))
    )
  }

  fn recreate_on_surface(_old: &Self, surface: &std::sync::Arc<WebGLSurface>, _width: u32, _height: u32) -> Result<std::sync::Arc<Self>, sourcerenderer_core::graphics::SwapchainError> {
    Ok(
      Arc::new(WebGLSwapchain::new(&surface))
    )
  }

  fn sample_count(&self) -> sourcerenderer_core::graphics::SampleCount {
    SampleCount::Samples1
  }

  fn format(&self) -> sourcerenderer_core::graphics::Format {
    Format::Unknown
  }

  fn surface(&self) -> &std::sync::Arc<WebGLSurface> {
    &self.surface
  }

  fn prepare_back_buffer(&self, semaphore: &Arc<WebGLSemaphore>) -> Option<Arc<WebGLRenderTargetView>> {
    todo!()
  }

  fn width(&self) -> u32 {
    self.width
  }

  fn height(&self) -> u32 {
    self.height
  }
}
