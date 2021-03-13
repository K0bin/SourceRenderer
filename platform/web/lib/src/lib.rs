mod utils;
mod web_engine;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use self::web_engine::WebEngine;
use web_sys::{EventTarget, HtmlCanvasElement};

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

#[wasm_bindgen(js_name = "render")]
pub fn render(engine: &mut WebEngine) {
  engine.render();
}
