use std::sync::{Arc, atomic::{AtomicU32, AtomicU64, Ordering}};

use log::trace;
use sourcerenderer_core::graphics::{Format, SampleCount, Surface, Swapchain, TextureInfo, TextureRenderTargetViewInfo, TextureShaderResourceView, TextureUsage};
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, WebGl2RenderingContext};

use crate::{GLThreadSender, WebGLBackend, WebGLDevice, WebGLTexture, WebGLTextureShaderResourceView, sync::WebGLSemaphore, texture::WebGLRenderTargetView, thread::WebGLThreadShader};

pub struct WebGLSurface {
  //canvas_element: HtmlCanvasElement

  selector: String,
  width: u32,
  height: u32
}

impl Surface for WebGLSurface {}

impl PartialEq for WebGLSurface {
  fn eq(&self, other: &Self) -> bool {
    //self.canvas_element == other.canvas_element
    self.selector == other.selector
  }
}

impl Eq for WebGLSurface {}

impl WebGLSurface {
  /*pub fn new(canvas_element: &HtmlCanvasElement) -> Self {
    Self {
      canvas_element: canvas_element.clone()
    }
  }*/

  pub fn new(selector: &str, document: &Document) -> Self {
    let canvas = Self::canvas_internal(selector, document);
    let width = canvas.width();
    let height = canvas.height();
    Self {
      selector: selector.to_string(),
      width,
      height
    }
  }

  fn canvas_internal(selector: &str, document: &Document) -> HtmlCanvasElement {
    let element = document.query_selector(selector).unwrap().unwrap();
    element.dyn_into::<HtmlCanvasElement>().unwrap()
  }

  pub fn canvas(&self, document: &Document) -> HtmlCanvasElement {
    Self::canvas_internal(&self.selector, document)
  }

  pub fn selector(&self) -> &str {
    &self.selector
  }
}

pub struct WebGLSwapchain {
  prepared_frame: AtomicU64,
  processed_frame: AtomicU64,
  surface: Arc<WebGLSurface>,
  width: u32,
  height: u32,
  backbuffer_view: Arc<WebGLRenderTargetView>
}

impl WebGLSwapchain {
  pub fn new(surface: &Arc<WebGLSurface>) -> Self {
    let backbuffer = Arc::new(WebGLTexture::new_internal(&TextureInfo {
      format: Format::Unknown,
      width: surface.width,
      height: surface.height,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET | TextureUsage::PRESENT,
    }));

    let view = Arc::new(WebGLRenderTargetView::new(&backbuffer, &TextureRenderTargetViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1
    }));

    Self {
      prepared_frame: AtomicU64::new(0),
      processed_frame: AtomicU64::new(0),
      surface: surface.clone(),
      width: surface.width,
      height: surface.height,
      backbuffer_view: view
    }
  }

  pub(crate) fn bump_frame(self: &Arc<Self>) {
    // Has to be called on the GL thread
    self.processed_frame.fetch_add(1, Ordering::SeqCst);
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

  fn prepare_back_buffer(&self, _semaphore: &Arc<WebGLSemaphore>) -> Option<Arc<WebGLRenderTargetView>> {
    while self.processed_frame.load(Ordering::SeqCst) + 1 < self.prepared_frame.load(Ordering::SeqCst) {
      // Block so we dont run too far ahead of the GL thread
    }

    self.prepared_frame.fetch_add(1, Ordering::SeqCst);
    Some(self.backbuffer_view.clone())
  }

  fn width(&self) -> u32 {
    self.width
  }

  fn height(&self) -> u32 {
    self.height
  }
}
