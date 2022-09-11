use std::sync::{Arc, atomic::{AtomicU64, Ordering}, Mutex, Condvar};

use sourcerenderer_core::graphics::{Format, SampleCount, Surface, Swapchain, Texture, TextureInfo, TextureViewInfo, TextureUsage, TextureDimension};
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, WebGl2RenderingContext};

use crate::{GLThreadSender, WebGLBackend, WebGLTexture, device::WebGLHandleAllocator, sync::WebGLSemaphore, texture::WebGLRenderTargetView, thread::WebGLTextureHandleView};

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

pub struct GLThreadSync {
  empty_mutex: Mutex<()>, // Use empty mutex + atomics so we never accidently call wait on the main thread if there's contention
  prepared_frame: AtomicU64,
  processed_frame: AtomicU64,
  condvar: Condvar,
}

pub struct WebGLSwapchain {
  sync: Arc<GLThreadSync>,
  surface: Arc<WebGLSurface>,
  width: u32,
  height: u32,
  backbuffer_view: Arc<WebGLRenderTargetView>,
  sender: GLThreadSender,
  allocator: Arc<WebGLHandleAllocator>,
}

impl WebGLSwapchain {
  pub fn new(surface: &Arc<WebGLSurface>, sender: &GLThreadSender, allocator: &Arc<WebGLHandleAllocator>) -> Self {
    let handle = allocator.new_texture_handle();
    let backbuffer = Arc::new(WebGLTexture::new(handle, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: surface.width,
      height: surface.height,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET,
      supports_srgb: false,
    }, sender));

    let view = Arc::new(WebGLRenderTargetView::new(&backbuffer, &TextureViewInfo::default()));

    Self {
      sync: Arc::new(GLThreadSync {
        empty_mutex: Mutex::new(()),
        condvar: Condvar::new(),
        prepared_frame: AtomicU64::new(0),
        processed_frame: AtomicU64::new(0),
      }),
      surface: surface.clone(),
      width: surface.width,
      height: surface.height,
      backbuffer_view: view,
      sender: sender.clone(),
      allocator: allocator.clone(),
    }
  }

  pub(crate) fn present(&self) {
    self.sync.prepared_frame.fetch_add(1, Ordering::SeqCst);

    let c_sync = self.sync.clone();
    let backbuffer_handle = self.backbuffer_view.texture().handle();
    let info = self.backbuffer_view.texture().info();
    let width = info.width as i32;
    let height = info.height as i32;
    self.sender.send(Box::new(move |device| {
      let mut rts: [Option<WebGLTextureHandleView>; 8] = Default::default();
      rts[0] = Some(WebGLTextureHandleView {
        texture: backbuffer_handle,
        array_layer: 0,
        mip: 0,
      });
      let read_fb = device.get_framebuffer(&rts, None, Format::Unknown);
      device.bind_framebuffer(WebGl2RenderingContext::DRAW_FRAMEBUFFER, None);
      device.bind_framebuffer(WebGl2RenderingContext::READ_FRAMEBUFFER, Some(&read_fb));
      device.blit_framebuffer(0, 0, width, height, 0, 0, width, height, WebGl2RenderingContext::COLOR_BUFFER_BIT, WebGl2RenderingContext::LINEAR);

      c_sync.processed_frame.fetch_add(1, Ordering::SeqCst);
      c_sync.condvar.notify_all();
    }));
  }
}

impl Swapchain<WebGLBackend> for WebGLSwapchain {
  fn recreate(old: &Self, _width: u32, _height: u32) -> Result<std::sync::Arc<Self>, sourcerenderer_core::graphics::SwapchainError> {
    Ok(
      Arc::new(WebGLSwapchain::new(&old.surface, &old.sender, &old.allocator))
    )
  }

  fn recreate_on_surface(old: &Self, surface: &std::sync::Arc<WebGLSurface>, _width: u32, _height: u32) -> Result<std::sync::Arc<Self>, sourcerenderer_core::graphics::SwapchainError> {
    Ok(
      Arc::new(WebGLSwapchain::new(&surface, &old.sender, &old.allocator))
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
    let guard = self.sync.empty_mutex.lock().unwrap();
    let _ = self.sync.condvar.wait_while(guard, |_| self.sync.processed_frame.load(Ordering::SeqCst) + 1 < self.sync.prepared_frame.load(Ordering::SeqCst)).unwrap();

    Some(self.backbuffer_view.clone())
  }

  fn transform(&self) -> sourcerenderer_core::Matrix4 {
    sourcerenderer_core::Matrix4::identity()
  }

  fn width(&self) -> u32 {
    self.width
  }

  fn height(&self) -> u32 {
    self.height
  }
}
