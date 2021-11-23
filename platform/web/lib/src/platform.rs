use std::{error::Error, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use log::warn;
use sourcerenderer_core::{Platform, platform::ThreadHandle};
use sourcerenderer_webgl::{WebGLBackend, WebGLInstance};
use web_sys::HtmlCanvasElement;

use crate::{io::WebIO, pool::WorkerPool, window::WebWindow};


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
    let thread_done = Arc::new(AtomicBool::new(true));
    let c_thread_done = thread_done.clone();
    self.pool.run_permanent(move || {
      // We need to set isDone to false on the thread because the thread only gets started when the
      // we return control of the main thread to the browser. So this could end up in a dead loop
      // where we end up waiting on a thread that will never be started.
      // This solution is pretty shit too if you're waiting for something calculated on the thread
      // but what can you do. ._.
      if !c_thread_done.swap(false, Ordering::SeqCst) {
        // Thread was joined before it even started
        warn!("Thread was joined before it was started!");
        return;
      }
      callback();
      c_thread_done.store(true, Ordering::SeqCst);
    }, Some(name)).unwrap();
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
