use std::{error::Error, sync::{Arc, atomic::AtomicBool}};

use sourcerenderer_core::Platform;
use sourcerenderer_webgl::{GLThreadReceiver, WebGLBackend, WebGLInstance};
use web_sys::{Document, HtmlCanvasElement};
#[macro_use]
use lazy_static;

use crate::{io::WebIO, threadpool::WorkerPool, window::WebWindow};

pub struct WebPlatform {
  window: WebWindow,
  instance: Arc<WebGLInstance>,
  pool: WorkerPool
}

impl WebPlatform {
  pub(crate) fn new(canvas_selector: &str) -> Self {

    let pool = WorkerPool::new(8).unwrap();

    rayon::ThreadPoolBuilder::new()
      .num_threads(8)
      .spawn_handler(|thread| Ok(pool.run(|| thread.run()).unwrap()))
      .build_global()
      .unwrap();

    Self {
      window: WebWindow::new(canvas_selector),
      instance: Arc::new(WebGLInstance::new()),
      pool
    }
  }
}

impl Platform for WebPlatform {
  type GraphicsBackend = WebGLBackend;
  type Window = WebWindow;
  type IO = WebIO;

  fn window(&self) -> &Self::Window {
    &self.window
  }

  fn create_graphics(&self, _debug_layers: bool) -> Result<Arc<WebGLInstance>, Box<dyn Error>> {
    Ok(self.instance.clone())
  }

  fn start_thread<F>(&self, name: &str, callback: F)
  where
      F: FnOnce(),
      F: Send + 'static {
    self.pool.run(callback).unwrap();
  }
}
