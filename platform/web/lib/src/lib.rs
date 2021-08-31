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
use game::Game;

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
}

#[macro_export]
macro_rules! console_log {
  // Note that this is using the `log` function imported above during
  // `bare_bones`
  ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen(js_name = "startEngine")]
pub fn start_engine(canvas: EventTarget) -> WebEngine {
  // must use extremely generic type here and to avoid typescript errors
  // when loading the wasm module on a web worker where DOM types dont exist
  utils::set_panic_hook();
  WebEngine::run(canvas.dyn_into::<HtmlCanvasElement>().unwrap())
}

#[wasm_bindgen(js_name = "gameWorkerMain")]
pub fn start_game(tick_rate: u32) -> Game {
  utils::set_panic_hook();
  Game::run(tick_rate)
}

#[wasm_bindgen(raw_module = "../../www/src/lib.ts")]
extern "C" {
  #[wasm_bindgen(js_name = "startGameWorker", catch)]
  pub fn start_game_worker() -> Result<Worker, JsValue>;
  #[wasm_bindgen(js_name = "startAssetWorker", catch)]
  pub fn start_asset_worker() -> Result<Worker, JsValue>;
}