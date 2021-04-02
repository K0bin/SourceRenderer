mod utils;
mod web_engine;
mod game;
mod renderer;

extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate legion;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use self::web_engine::WebEngine;
use web_sys::{EventTarget, HtmlCanvasElement, Worker};
use self::renderer::Renderer;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen(js_name = "startEngine")]
pub fn start_engine(canvas: EventTarget) -> WebEngine {
  // must use extremely generic type here and to avoid typescript errors
  // when loading the wasm module on a web worker where DOM types dont exist
  WebEngine::run(canvas.dyn_into::<HtmlCanvasElement>().unwrap())
}

/*#[wasm_bindgen(js_name = "startGameWorker")]
pub fn start_game_worker() {
  //Game::new()
  unimplemented!()
}*/

#[wasm_bindgen(raw_module = "../../www/src/lib.ts")]
extern "C" {
  #[wasm_bindgen(js_name = "startGameWorker", catch)]
  pub fn start_game_worker() -> Result<Worker, JsValue>;
  #[wasm_bindgen(js_name = "startAssetWorker", catch)]
  pub fn start_asset_worker() -> Result<Worker, JsValue>;
}