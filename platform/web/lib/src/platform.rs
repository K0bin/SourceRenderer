use std::{cell::RefCell, error::Error, sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}}, thread::Thread};

use crossbeam_channel::unbounded;
use sourcerenderer_core::Platform;
use sourcerenderer_webgl::{GLThreadReceiver, WebGLBackend, WebGLInstance, WebGLThreadDevice};
use web_sys::{Document, HtmlCanvasElement, Worker};
#[macro_use]
use lazy_static;

use crate::{console_log, io::WebIO, pool::WorkerPool, window::WebWindow};

pub struct WebPlatform {
  window: WebWindow,
  instance: Arc<WebGLInstance>,
  pool: WorkerPool
}

impl WebPlatform {
  pub(crate) fn new(canvas_selector: &str, worker_pool: WorkerPool) -> Self {
    Self {
      window: WebWindow::new(canvas_selector),
      instance: Arc::new(WebGLInstance::new()),
      pool: worker_pool
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
