use js_sys::DataView;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue, JsCast};
use web_sys::DedicatedWorkerGlobalScope;

pub const BUFFER_USAGE_VERTX: u32 = 1;
pub const BUFFER_USAGE_INDEX: u32 = 2;
pub const BUFFER_USAGE_TRANSFER_SRC: u32 = 3;

pub const MEMORY_USAGE_GPU_ONLY: u32 = 0;
pub const MEMORY_USAGE_GPU_TO_CPU: u32 = 1;
pub const MEMORY_USAGE_CPU_TO_GPU: u32 = 2;
pub const MEMORY_USAGE_CPU_ONLY: u32 = 3;


#[wasm_bindgen]
extern "C" {
  pub type WebGLCreateBufferCommand;

  #[wasm_bindgen(constructor)]
  pub fn new(id: u32, size: u32) -> WebGLCreateBufferCommand;

  pub type WebGLSetBufferDataCommand;

  #[wasm_bindgen(constructor)]
  pub fn new(id: u32, data_view: &DataView) -> WebGLSetBufferDataCommand;

  pub type WebGLDestroyBufferCommand;

  #[wasm_bindgen(constructor)]
  pub fn new(id: u32) -> WebGLDestroyBufferCommand;
}

pub fn send_message(msg: &JsValue) {
  let global = js_sys::global();
  let worker_global = global.dyn_into::<DedicatedWorkerGlobalScope>().unwrap();
  worker_global.post_message(msg).unwrap();
}
