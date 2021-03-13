
use wasm_bindgen::{prelude::*, closure::Closure, JsCast};
use web_sys::{HtmlCanvasElement, Worker, Window, window};
use std::{rc::Rc, cell::RefCell};
pub struct Renderer {
  is_running: bool
}

impl Renderer {
  pub fn new() -> Self {
    Self {
      is_running: true
    }
  }

  pub fn is_running(&self) -> bool {
    self.is_running
  }

  pub fn stop(&mut self) {
    self.is_running = false;
  }

  pub fn render(&mut self) {

  }
}
