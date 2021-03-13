use wasm_bindgen::{prelude::*, closure::Closure, JsCast};
use web_sys::{HtmlCanvasElement, Worker, Window, window};
use std::{rc::Rc, cell::RefCell};
use crate::Renderer;

#[wasm_bindgen]
pub struct WebEngine {
  game_worker: Worker,
  asset_worker: Worker,
  renderer: Rc<RefCell<Renderer>>,
  _render_callback: Rc<RefCell<Option<Closure<dyn FnMut()>>>>
}

impl WebEngine {
  pub fn run(canvas: HtmlCanvasElement) -> Self {
    let game_worker = Worker::new("./game_worker.js").unwrap();
    let asset_worker = Worker::new("./asset_worker.js").unwrap();

    let renderer = Rc::new(RefCell::new(Renderer::new()));

    let closure = Rc::new(RefCell::new(Option::<Closure<dyn FnMut()>>::None));
    let c_closure = closure.clone();
    let c_renderer = renderer.clone();
    *closure.borrow_mut() = Some(Closure::wrap(Box::new(move || {
      let mut renderer_mut = c_renderer.borrow_mut();
      if !renderer_mut.is_running() {
        let _ = c_closure.borrow_mut().take();
        return;
      }

      renderer_mut.render();
      request_animation_frame(c_closure.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(closure.borrow().as_ref().unwrap());

    Self {
      game_worker,
      asset_worker,
      renderer,
      _render_callback: closure
    }
  }
}

impl Drop for WebEngine {
  fn drop(&mut self) {
    self.renderer.borrow_mut().stop();
  }
}

fn request_animation_frame(callback: &Closure<FnMut()>) {
  window()
    .unwrap()
    .request_animation_frame(callback.as_ref().unchecked_ref());
}
