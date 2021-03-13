use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, Worker, Window, window};

#[wasm_bindgen]
pub struct WebEngine {
  game_worker: Worker,
  asset_worker: Worker
}

impl WebEngine {
  pub fn run(canvas: HtmlCanvasElement) -> Self {
    let game_worker = Worker::new("./game_worker.js").unwrap();
    let asset_worker = Worker::new("./asset_worker.js").unwrap();
    println!("test");

    Self {
      game_worker,
      asset_worker
    }
  }

  pub fn render(&mut self) {
    // render frame
  }
}
