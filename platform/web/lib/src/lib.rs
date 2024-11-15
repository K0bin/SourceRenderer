mod utils;
//mod pool;
//mod platform;
//mod io;
//mod window;
//mod async_io_worker;

extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rayon;
#[macro_use]
extern crate lazy_static;
extern crate crossbeam_channel;

use std::cell::RefCell;
use std::cell::RefMut;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use sourcerenderer_core::Platform;
use sourcerenderer_core::platform::Window;
use sourcerenderer_engine::Engine;
//use sourcerenderer_webgl::WebGLSwapchain;
//use crate::pool::WorkerPool;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::window;
use web_sys::{EventTarget, HtmlCanvasElement, Worker};
//use self::platform::WebPlatform;
//use sourcerenderer_webgl::WebGLThreadDevice;
use crossbeam_channel::unbounded;


#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    /*// The `console.log` is quite polymorphic, so we can bind it with multiple
    // signatures. Note that we need to use `js_name` to ensure we always call
    // `log` in JS.
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log_u32(a: u32);

    // Multiple arguments too!
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log_many(a: &str, b: &str);

    // Multiple arguments too!
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn logv(a: &JsValue);*/
}

#[macro_export]
macro_rules! console_log {
  // Note that this is using the `log` function imported above during
  // `bare_bones`
  ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
}


struct EngineWrapper {
  //engine: Engine<WebPlatform>,
  _unused: u32
}

#[wasm_bindgen(js_name = "startEngine")]
pub fn start_engine(canvas: HtmlCanvasElement) -> usize {
  utils::set_panic_hook();

  console_log::init_with_level(log::Level::Trace).unwrap();

  //console_log!("Initializing platform");
  //let platform = WebPlatform::new(canvas, worker_pool);

  console_log!("Initializing engine");
  /*let engine = Engine::run(&platform, );
  let device = engine.device().clone();
  let surface = engine.surface().clone();
  let receiver = device.receiver();*/
  //let window = platform.window();
  //let document = window.document();
  //let thread_device = WebGLThreadDevice::new(receiver, &surface, document);

  let wrapper = Box::new(RefCell::new(EngineWrapper {
    //gl_device: thread_device,
    //engine
    _unused: 1337
  }));
  Box::into_raw(wrapper) as usize
}


fn engine_from_usize<'a>(engine_ptr: usize) -> RefMut<'a, EngineWrapper> {
  assert_ne!(engine_ptr, 0);
  unsafe {
    let ptr = std::mem::transmute::<usize, *mut RefCell<EngineWrapper>>(engine_ptr);
    let engine: RefMut<EngineWrapper> = (*ptr).borrow_mut();
    let engine_ref = std::mem::transmute::<RefMut<EngineWrapper>, RefMut<'a, EngineWrapper>>(engine);
    engine_ref
  }
}

#[wasm_bindgen]
pub struct RayonInitialization {
  rayon_threads: u32,
  ready_thread_counter: Arc<AtomicU32>
}

#[wasm_bindgen(js_name = "engineFrame")]
pub fn engine_frame(engine: usize) -> bool {
  let mut wrapper = engine_from_usize(engine);
  /*wrapper.engine.frame();
  wrapper.gl_device.process();*/
  true
}
