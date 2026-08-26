use io::WebIO;
use js_sys::Uint8Array;
use log::info;
use platform::WebPlatform;
use sourcerenderer_engine::{Engine as ActualEngine, EngineLoopFuncResult};
use sourcerenderer_game::GamePlugin;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use web_sys::{OffscreenCanvas, WorkerNavigator};

mod io;
mod platform;
mod utils;
mod window;

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
pub async fn start_engine(canvas: OffscreenCanvas) -> Engine {
    utils::set_panic_hook();
    console_log::init_with_level(log::Level::Trace).unwrap();

    info!("Initializing platform");
    let platform = WebPlatform::new_on_worker(canvas).await;

    info!("Initializing engine");
    let engine = ActualEngine::run::<_, WebIO, WebPlatform>(
        platform.window(),
        GamePlugin::<WebIO>::default(),
    );

    let wrapper = Engine {
        engine: Some(engine),
    };
    wrapper
}

#[wasm_bindgen(js_name = "hasRenderThread")]
pub fn has_render_thread() -> bool {
    true
}

#[wasm_bindgen(raw_module = "../../www/src/web_glue.ts")]
extern "C" {
    #[wasm_bindgen(js_name = "fetchAsset", catch)]
    pub async extern "C" fn fetch_asset(path: &str) -> Result<Uint8Array, JsValue>;
    #[wasm_bindgen(js_name = "fetchAssetHead", catch)]
    pub async extern "C" fn fetch_asset_head(path: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_name = "fetchAssetRange", catch)]
    pub async extern "C" fn fetch_asset_range(
        path: &str,
        offset: u32,
        length: u32,
    ) -> Result<Uint8Array, JsValue>;
}

// Fix missing TLS exports
thread_local! {
    static DUMMY: std::cell::RefCell<i32> = std::cell::RefCell::new(0);
}
#[wasm_bindgen]
pub fn __force_tls_initialization() {
    DUMMY.with(|dummy| {
        *dummy.borrow_mut() += 1;
    });
}
