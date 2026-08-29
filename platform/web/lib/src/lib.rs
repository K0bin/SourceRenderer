use crate::window::WebWindow;
use io::WebIO;
use js_sys::{JsString, Uint8Array};
use log::info;
use platform::WebPlatform;
use sourcerenderer_core::Vec2;
use sourcerenderer_engine::{
    ButtonState, Engine as ActualEngine, EngineLoopFuncResult, Key, KeyCode, KeyboardInput,
    MouseMotion, WindowState,
};
use sourcerenderer_game::GamePlugin;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use web_sys::OffscreenCanvas;

mod io;
mod platform;
mod utils;
mod window;

#[wasm_bindgen]
pub struct Engine {
    engine: Option<ActualEngine>,
    platform: WebPlatform,
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

    #[wasm_bindgen(js_name = "windowResized")]
    pub fn window_resized(&mut self, width: u32, height: u32) {
        let engine = if let Some(engine) = self.engine.as_mut() {
            engine
        } else {
            log::error!("Engine has been stopped.");
            return;
        };

        engine.window_changed::<WebPlatform>(WindowState::Window(width, height));
    }

    #[wasm_bindgen(js_name = "mouseMoved")]
    pub fn mouse_moved(&mut self, delta_x: f32, delta_y: f32) {
        let engine = if let Some(engine) = self.engine.as_mut() {
            engine
        } else {
            log::error!("Engine has been stopped.");
            return;
        };
        engine.dispatch_mouse_motion(MouseMotion {
            delta: Vec2::new(delta_x, delta_y),
        });
    }

    #[wasm_bindgen(js_name = "keyboardEvent")]
    pub fn keyboard_event(&mut self, down: bool, key: &JsString) {
        let engine = if let Some(engine) = self.engine.as_mut() {
            engine
        } else {
            log::error!("Engine has been stopped.");
            return;
        };
        let key_str = key.as_string();
        if key_str.is_none() {
            return;
        }
        let key_code = js_key_code_to_engine_key_code(key_str.as_ref().unwrap());
        if key_code.is_none() {
            return;
        }
        engine.dispatch_keyboard_input(KeyboardInput {
            key_code: key_code.unwrap(),
            logical_key: Key::Dead(None),
            state: ButtonState::Released,
            window: engine.get_window_dummy_entity(),
            repeat: false,
            text: None,
        });
    }

    #[wasm_bindgen(js_name = "isMouseLocked")]
    pub fn is_mouse_locked(&self) -> bool {
        let engine = if let Some(engine) = self.engine.as_ref() {
            engine
        } else {
            log::error!("Engine has been stopped.");
            return false;
        };
        engine.is_mouse_locked()
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
        platform,
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

fn js_key_code_to_engine_key_code(key: &str) -> Option<KeyCode> {
    match key {
        "KeyW" => Some(KeyCode::KeyW),
        "KeyA" => Some(KeyCode::KeyA),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyE" => Some(KeyCode::KeyE),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ControlLeft" => Some(KeyCode::ControlLeft),
        "Escape" => Some(KeyCode::Escape),
        _ => None,
    }
}
