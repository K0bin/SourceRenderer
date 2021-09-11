use std::{cell::RefCell, error::Error, sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}}, thread::Thread};

use sourcerenderer_core::Platform;
use sourcerenderer_webgl::{GLThreadReceiver, WebGLBackend, WebGLInstance, WebGLThreadDevice};
use web_sys::{Document, HtmlCanvasElement, Worker};
#[macro_use]
use lazy_static;

use crate::{console_log, io::WebIO, worker_pool::WorkerPool, window::WebWindow};

pub struct WebPlatform {
  window: WebWindow,
  instance: Arc<WebGLInstance>,
  pool: Arc<WorkerPool>
}

impl WebPlatform {
  pub(crate) fn new(canvas_selector: &str, worker_pool: WorkerPool) -> Self {

    console_log!("Initializing worker pool");
    //let pool = Arc::new(WorkerPool::new(16));
    let pool = Arc::new(worker_pool);

    let rayon_thread_count = 4;
    let rayon_init = Arc::new(AtomicBool::new(false));
    let c_rayon_init = rayon_init.clone();
    let c_pool = pool.clone();
    pool.run(move || {
      // Has to be done on a worker thread because of the blocking restriction
      console_log!("Initializing rayon thread pool");
      rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_thread_count)
        .spawn_handler(|thread| Ok(c_pool.run(move || thread.run())))
        .build_global()
        .unwrap();
      console_log!("Rayon initialized");
      c_rayon_init.store(true, Ordering::SeqCst)
    });

    //while !rayon_init.load(Ordering::SeqCst) {}

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
    self.pool.run(callback);
  }
}
