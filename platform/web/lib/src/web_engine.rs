use wasm_bindgen::{prelude::*, closure::Closure, JsCast};
use web_sys::{HtmlCanvasElement, Worker, window};
use std::{rc::Rc, cell::RefCell};
use crate::{Renderer, start_asset_worker, start_game_worker, log};

#[wasm_bindgen]
pub struct WebEngine {
  _game_worker: Worker,
  _asset_worker: Worker,
  renderer: Rc<RefCell<Renderer>>,
  _frame: u32,
  _render_callback: Rc<RefCell<Option<Closure<dyn FnMut()>>>>
}

impl WebEngine {
  pub fn run(_canvas: HtmlCanvasElement) -> Self {
    let game_worker = start_game_worker().unwrap();
    let asset_worker = start_asset_worker().unwrap();

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
      //renderer_mut.frame += 1;
      request_animation_frame(c_closure.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(closure.borrow().as_ref().unwrap());

    Self {
      _game_worker: game_worker,
      _asset_worker: asset_worker,
      renderer,
      _frame: 0,
      _render_callback: closure
    }
  }
}

impl Drop for WebEngine {
  fn drop(&mut self) {
    self.renderer.borrow_mut().stop();
  }
}

fn request_animation_frame(callback: &Closure<dyn FnMut()>) {
  window()
    .unwrap()
    .request_animation_frame(callback.as_ref().unchecked_ref()).unwrap();
}
