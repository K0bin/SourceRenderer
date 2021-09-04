mod utils;
mod threadpool;
mod platform;
mod io;
mod window;

extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate legion;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rayon;
#[macro_use]
extern crate lazy_static;

use std::cell::RefCell;
use std::rc::Rc;

use sourcerenderer_core::Platform;
use sourcerenderer_core::platform::Window;
use sourcerenderer_engine::Engine;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::window;
use web_sys::{EventTarget, HtmlCanvasElement, Worker};
use self::platform::WebPlatform;
use sourcerenderer_webgl::WebGLThreadDevice;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    // The `console.log` is quite polymorphic, so we can bind it with multiple
    // signatures. Note that we need to use `js_name` to ensure we always call
    // `log` in JS.
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log_u32(a: u32);

    // Multiple arguments too!
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log_many(a: &str, b: &str);

    // Multiple arguments too!
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn logv(a: &JsValue);
}

#[macro_export]
macro_rules! console_log {
  // Note that this is using the `log` function imported above during
  // `bare_bones`
  ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen(js_name = "startEngine")]
pub fn start_engine(canvas_selector: &str) {
  utils::set_panic_hook();

  console_log!("Initializing platform");
  let platform = WebPlatform::new(canvas_selector);

  console_log!("Initializing engine");
  let mut engine = Engine::run(&platform);
  console_log!("Initialized engine");
  let device = engine.device().clone();
  console_log!("Got device");
  let surface = engine.surface().clone();
  console_log!("Got surface");
  let receiver = device.receiver();
  let window = platform.window();
  let web_window = window.window();
  let document = window.document();
  let mut thread_device = WebGLThreadDevice::new(receiver, &surface, document);


  let closure = Rc::new(RefCell::new(Option::<Closure<dyn FnMut()>>::None));
  let c_closure = closure.clone();
  let c_web_window = web_window.clone();
  let c_swapchain = window.create_swapchain(true, &device, &surface); // Returns the same swapchain for WebWindow
  *closure.borrow_mut() = Some(Closure::wrap(Box::new(move || {
    // TODO: Sample inputs

    let exit = false;
    if exit {
      let _ = c_closure.borrow_mut().take();
      return;
    }

    thread_device.process();
    c_swapchain.bump_frame();

    c_web_window.request_animation_frame((c_closure.borrow().as_ref().unwrap()).as_ref().unchecked_ref()).unwrap();
  }) as Box<dyn FnMut()>));

  web_window.request_animation_frame(closure.borrow().as_ref().unwrap().as_ref().unchecked_ref()).unwrap();

  std::mem::forget(closure);
}
