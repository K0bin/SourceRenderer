// Silences warnings from the compiler about Work.func and child_entry_point
// being unused when the target is not wasm.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

//! A small module that's intended to provide an example of creating a pool of
//! web workers which can be used to execute `rayon`-style work.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};
use web_sys::{ErrorEvent, Event, Worker, WorkerOptions};

use crate::console_log;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "startThread")]
    fn start_thread(work: u32);
}

#[wasm_bindgen]
pub struct WorkerPool {
}

struct Work {
    func: Box<dyn FnOnce() + Send>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkerType<'a> {
    Permanent,
    PermenantNamed(&'a str),
    Temporary
}

#[wasm_bindgen]
impl WorkerPool {
    /// Creates a new `WorkerPool` which immediately creates `initial` workers.
    ///
    /// The pool created here can be used over a long period of time, and it
    /// will be initially primed with `initial` workers. Currently workers are
    /// never released or gc'd until the whole pool is destroyed.
    ///
    /// # Errors
    ///
    /// Returns any error that may happen while a JS web worker is created and a
    /// message is sent to it.
    #[wasm_bindgen(constructor)]
    pub fn new(initial: usize) -> Result<WorkerPool, JsValue> {
        let pool = WorkerPool {
        };
        Ok(pool)
    }
}

impl WorkerPool {
    /// Executes `f` in a web worker.
    ///
    /// This pool manages a set of web workers to draw from, and `f` will be
    /// spawned quickly into one if the worker is idle. If no idle workers are
    /// available then a new web worker will be spawned.
    ///
    /// # Errors
    ///
    /// If an error happens while spawning a web worker or sending a message to
    /// a web worker, that error is returned.
    pub fn run(&self, f: impl FnOnce() + Send + 'static) -> Result<(), JsValue> {
        let work = Box::new(Work { func: Box::new(f) });
        let ptr = Box::into_raw(work);
        start_thread(ptr as u32);
        Ok(())
    }

    pub fn run_permanent(&self, f: impl FnOnce() + Send + 'static, name: Option<&str>) -> Result<(), JsValue> {
        let work = Box::new(Work { func: Box::new(f) });
        let ptr = Box::into_raw(work);
        start_thread(ptr as u32);
        Ok(())
    }
}

/// Entry point invoked by `worker.js`, a bit of a hack but see the "TODO" above
/// about `worker.js` in general.
#[wasm_bindgen]
pub fn child_entry_point(ptr: u32) -> Result<(), JsValue> {
    let ptr = unsafe { Box::from_raw(ptr as *mut Work) };
    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
    (ptr.func)();
    global.post_message(&JsValue::undefined())?;
    Ok(())
}
