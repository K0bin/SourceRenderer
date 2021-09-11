#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}};

use web_sys::{DedicatedWorkerGlobalScope, Event, Worker};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen::{self, JsValue};
use crossbeam_channel::{self, Receiver, Sender, TryRecvError, unbounded};

use crate::console_log;

pub enum Work {
  Work(Box<dyn FnOnce()>),
  Terminate
}

#[wasm_bindgen]
pub struct WorkerPool {
  sender: Sender<Work>,
  active_workers: Arc<AtomicU32>
}

#[wasm_bindgen]
impl WorkerPool {
  #[wasm_bindgen(constructor)]
  pub fn new(threads: u32, callback: &js_sys::Function) -> Self {

    let active_workers = Arc::new(AtomicU32::new(0));
    let (sender, receiver) = unbounded::<Work>();

    // UGLY HACK
    console_log!("Starting {} threads", threads);
    let is_initialized = Box::new(AtomicBool::new(false));
    let is_initialized_ptr = Box::into_raw(is_initialized);

    let c_callback = callback.clone();
    let c_active_workers = active_workers.clone();
    let callback = Closure::wrap(Box::new(move |_event: Event| {
      let active = c_active_workers.fetch_add(1, Ordering::SeqCst) + 1;

      let is_initialized = unsafe { &*(is_initialized_ptr as *const AtomicBool) };
      if active == threads && !is_initialized.load(Ordering::SeqCst) {
        is_initialized.store(true, Ordering::SeqCst);
        console_log!("All threads started!");
        c_callback.call0(&JsValue::null()).unwrap();
      }
    }) as Box<dyn FnMut(Event)>);


    //let active_workers = Arc::new(AtomicU32::new(0));
    for _ in 0..threads {
      let worker = Worker::new("./worker.js").unwrap();
      
      let init = Box::new(WorkerInitMsg {
        receiver: receiver.clone(),
        active_workers: active_workers.clone()
      });
      let init_ptr = Box::into_raw(init);

      let array = js_sys::Array::new();
      array.push(&wasm_bindgen::module());
      array.push(&wasm_bindgen::memory());
      array.push(&JsValue::from(init_ptr as u32));
      worker.post_message(&array).unwrap();
      worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));
      std::mem::forget(worker);
    }
    std::mem::forget(callback);
    Self {
      sender: sender.clone(),
      active_workers: active_workers.clone()
    }
  }

  pub(crate) fn run<F>(&self, work: F)
  where
    F: FnOnce() + 'static {
    self.sender.send(Work::Work(Box::new(work))).unwrap();
  }
}

impl Drop for WorkerPool {
  fn drop(&mut self) {
    //panic!("AA");
    console_log!("Dropping worker pool");
    let mut active_workers = self.active_workers.load(Ordering::SeqCst);
    self.sender.send(Work::Terminate).unwrap();
    while active_workers > 0 {
      let new_active_workers = self.active_workers.load(Ordering::SeqCst);
      if active_workers != new_active_workers {
        self.sender.send(Work::Terminate).unwrap();
        active_workers = new_active_workers;
      }
    }
    console_log!("Worker pool dropped");
  }
}

pub struct WorkerInitMsg {
  receiver: Receiver<Work>,
  active_workers: Arc<AtomicU32>
}

#[wasm_bindgen]
pub fn worker_callback(init_ptr: u32) {
  let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
  let WorkerInitMsg { receiver, active_workers } = unsafe { *Box::<WorkerInitMsg>::from_raw(init_ptr as *mut WorkerInitMsg) };
  console_log!("Worker initialized");
  global.post_message(&JsValue::from(0u32)).unwrap(); // Send an empty message to signal that the worker is ready

  let mut terminate = false;
  'main_loop: loop {
    let work: Work = {
      let try_work = receiver.try_recv();
      if let Err(TryRecvError::Empty) = try_work {
        if terminate {
          break 'main_loop;
        } else {
          receiver.recv().unwrap()
        }
      } else {
        try_work.unwrap()
      }
    };
    match work {
      Work::Work(work) => {
        work();
      }
      Work::Terminate => {
        terminate = true;
        console_log!("Draining worker for termination");
      }
    }
  }
  console_log!("Worker terminated");
  active_workers.fetch_sub(1, Ordering::SeqCst);
}

