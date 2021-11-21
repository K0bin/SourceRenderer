use std::{cell::RefCell, error::Error, sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}}, thread::Thread};

use crossbeam_channel::unbounded;
use sourcerenderer_core::{Platform, platform::ThreadHandle};
use sourcerenderer_webgl::{GLThreadReceiver, WebGLBackend, WebGLInstance, WebGLThreadDevice};
use web_sys::{Document, HtmlCanvasElement, Worker};
use async_channel::Sender;
#[macro_use]
use lazy_static;

use crate::{async_io_worker::AsyncIOTask, console_log, io::WebIO, pool::WorkerPool, window::WebWindow};


pub struct WebPlatform {
  window: WebWindow,
  instance: Arc<WebGLInstance>,
  pool: WorkerPool
}

impl WebPlatform {
  pub(crate) fn new(canvas: HtmlCanvasElement, worker_pool: WorkerPool) -> Self {
    crate::io::init_global_io(&worker_pool);
    Self {
      window: WebWindow::new(canvas),
      instance: Arc::new(WebGLInstance::new()),
      pool: worker_pool
    }
  }
}

impl Platform for WebPlatform {
  type GraphicsBackend = WebGLBackend;
  type Window = WebWindow;
  type IO = WebIO;
  type ThreadHandle = BusyWaitThreadHandle;

  fn window(&self) -> &Self::Window {
    &self.window
  }

  fn create_graphics(&self, _debug_layers: bool) -> Result<Arc<WebGLInstance>, Box<dyn Error>> {
    Ok(self.instance.clone())
  }

  fn start_thread<F>(&self, name: &str, callback: F) -> Self::ThreadHandle
  where
      F: FnOnce(),
      F: Send + 'static {
    let thread_done = Arc::new(AtomicBool::new(false));
    let c_thread_done = thread_done.clone();
    self.pool.run_permanent(move || {
      callback();
      c_thread_done.store(true, Ordering::SeqCst);
    }).unwrap();
    BusyWaitThreadHandle(thread_done)
  }
}

pub struct BusyWaitThreadHandle(Arc<AtomicBool>);
impl ThreadHandle for BusyWaitThreadHandle {
  fn join(self) {
    // As with everything Web related, this is awful
    while !self.0.load(Ordering::SeqCst) {}
  }
}
