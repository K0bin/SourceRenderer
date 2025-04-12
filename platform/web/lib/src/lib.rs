use io::WebIO;
use js_sys::Uint8Array;
use log::info;
use platform::WebPlatform;
use sourcerenderer_engine::{Engine as ActualEngine, EngineLoopFuncResult};
use sourcerenderer_game::GamePlugin;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};
use web_sys::{Navigator, OffscreenCanvas};

mod platform;
mod window;
mod io;
mod utils;

#[wasm_bindgen]
pub struct Engine {
    engine: Option<ActualEngine>,
}

#[wasm_bindgen]
impl Engine {
    pub fn frame(&mut self) {
        let result: EngineLoopFuncResult;
        if let Some(engine) = self.engine.as_mut() {
          result = engine.frame();
        } else {
            log::error!("Engine has been stopped.");
            return;
        }
        if result == EngineLoopFuncResult::Exit {
            self.engine = None;
        }
    }
}


#[wasm_bindgen(js_name = "startEngine")]
pub async fn start_engine(navigator: Navigator, canvas: OffscreenCanvas) -> Engine {
  utils::set_panic_hook();

  console_log::init_with_level(log::Level::Trace).unwrap();

  info!("Initializing platform");
  let platform = WebPlatform::new(navigator, canvas).await;

  info!("Initializing engine");
  let engine = ActualEngine::run::<_, WebIO, WebPlatform>(platform.window(), GamePlugin);

  let wrapper = Engine {
    engine: Some(engine),
  };
  wrapper
}



#[wasm_bindgen(js_name = "module")]
pub fn module() -> js_sys::WebAssembly::Module {
  js_sys::WebAssembly::Module::from(wasm_bindgen::module())
}

#[wasm_bindgen(module = "/src/web_glue.ts")]
extern "C" {
  #[wasm_bindgen(js_name = "fetchAsset", catch)]
  pub async fn fetch_asset(path: &str) -> Result<Uint8Array, JsValue>;
}
