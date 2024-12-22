use log::info;
use platform::WebPlatform;
use sourcerenderer_engine::Engine as ActualEngine;
use sourcerenderer_game::GamePlugin;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{Navigator, OffscreenCanvas};

mod platform;
mod window;
mod io;
mod utils;

#[wasm_bindgen]
pub struct Engine {
    engine: ActualEngine
}

#[wasm_bindgen]
impl Engine {
    pub fn frame(&mut self) {
        self.engine.frame();
    }
}


#[wasm_bindgen(js_name = "startEngine")]
pub async fn start_engine(navigator: Navigator, canvas: OffscreenCanvas) -> Engine {
  utils::set_panic_hook();

  console_log::init_with_level(log::Level::Trace).unwrap();

  info!("Initializing platform");
  let platform = WebPlatform::new(navigator, canvas).await;

  info!("Initializing engine");
  let engine = ActualEngine::run(&platform, GamePlugin::<WebPlatform>::default());

  let wrapper = Engine {
    engine
  };
  wrapper
}
