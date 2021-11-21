use std::sync::{Mutex, Condvar, Arc, MutexGuard};

use async_channel::{Receiver, Sender, unbounded};
use sourcerenderer_core::Platform;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{DedicatedWorkerGlobalScope, WorkerGlobalScope, Response};
use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen_futures::JsFuture;

use crate::WorkerPool;
use crate::platform::WebPlatform;


#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsyncIOTaskError {
  InProgress,
  Error(String)
}

pub(crate) struct AsyncIOTask {
  pub path: String,
  pub cond_var: Condvar,
  pub result: Mutex<Result<Box<[u8]>, AsyncIOTaskError>>
}

impl AsyncIOTask {
  pub fn new(path: &str) -> Arc<Self> {
    Arc::new(Self {
      path: path.to_string(),
      cond_var: Condvar::new(),
      result: Mutex::new(Err(AsyncIOTaskError::InProgress))
    })
  }

  pub fn wait_for_result(&self) -> MutexGuard<Result<Box<[u8]>, AsyncIOTaskError>> {
    let mut guard = self.result.lock().unwrap();
    while guard.as_ref().err() == Some(&AsyncIOTaskError::InProgress) {
      guard = self.cond_var.wait(guard).unwrap();
    }
    guard
  }
}

pub(crate) fn start_worker(worker_pool: &WorkerPool) -> Sender<Arc<AsyncIOTask>> {
  crate::console_log!("Starting async worker");
  let (task_sender, task_receiver) = unbounded::<Arc<AsyncIOTask>>();
  worker_pool.run_permanent(move || {
    crate::console_log!("Starting async worker thread");
    let future = process(task_receiver);
    wasm_bindgen_futures::spawn_local(future);
  }).unwrap();
  task_sender
}

async fn process(task_receiver: Receiver<Arc<AsyncIOTask>>) {
  crate::console_log!("Started async worker");
  loop {
    let task = task_receiver.recv().await.unwrap();
    let result = handle_fetch_task(task.path.clone()).await;
    let mut result_guard = task.result.lock().unwrap();
    if let Ok(result) = result {
      *result_guard = Ok(result);
    } else {
      *result_guard = Err(AsyncIOTaskError::Error(format!("{:?}", result.err().unwrap())));
    }
    task.cond_var.notify_all();
  }
}

async fn handle_fetch_task(url: String) -> Result<Box<[u8]>, JsValue> {
  let global = js_sys::global();
  let worker_global = global.dyn_into::<DedicatedWorkerGlobalScope>().unwrap();
  let response_js_obj = JsFuture::from(worker_global.fetch_with_str(&url)).await?;
  let response: Response = response_js_obj.dyn_into()?;
  if !response.ok() {
    return Err(JsValue::from_str(&response.status_text()));
  }
  let buffer_js_obj = JsFuture::from(response.array_buffer()?).await?;
  let typed_array: Uint8Array = Uint8Array::new(&buffer_js_obj);
  let mut buffer = Vec::<u8>::with_capacity(typed_array.length() as usize);
  assert!(buffer.capacity() >= typed_array.length() as usize);
  unsafe {
    typed_array.raw_copy_to_ptr(buffer.as_mut_ptr());
    buffer.set_len(typed_array.length() as usize);
  }
  Ok(Vec::into_boxed_slice(buffer))
}