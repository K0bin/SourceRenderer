use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};

use js_sys::WebAssembly;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::{JsCast as _, JsValue, wasm_bindgen};

// Wasm thread
pub mod thread {
    use super::*;

    pub struct JoinHandle<T: Send>(Arc<ThreadShared<T>>);
    impl<T: Send> JoinHandle<T> {
        pub fn join(self) -> std::thread::Result<T> {
            let guard = self.0.state.lock().unwrap();
            let mut finished_guard = self.0.cond_var.wait_while(guard, |state| {
                let done = match state {
                    ThreadState::Started => false,
                    ThreadState::Finished(_) => true,
                    ThreadState::Initialized => {
                        log::warn!("Thread has not yet started execution. The event loop probably hasn't returned yet. This could be a deadlock.");
                        false
                    },
                    _ => panic!("Thread was already joined."),
                };
                !done
            }).unwrap();

            let finished_state = std::mem::replace(&mut *finished_guard, ThreadState::Joined);
            if let ThreadState::Finished(data) = finished_state {
                Ok(data)
            } else {
                unreachable!()
            }
        }

        pub fn is_finished(&self) -> bool {
            let guard = self.0.state.lock().unwrap();
            match &*guard {
                ThreadState::Finished(_) => true,
                _ => false,
            }
        }
    }

    pub fn spawn<F, T>(f: F)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        //spawn_with_js_val(|_| f(), JsValue::null())
        unimplemented!()
    }

    pub fn spawn_with_js_val<F, TF, T>(f: F, data: JsValue, name: Option<&str>) -> JoinHandle<T>
    where
        F: FnOnce(JsValue) -> TF + Send + 'static,
        TF: Future<Output = T> + 'static,
        T: Send + Unpin + 'static,
    {
        log::info!("Starting WASM thread");

        let shared = Arc::new(ThreadShared {
            state: Mutex::new(ThreadState::Initialized),
            cond_var: Condvar::new(),
        });

        let c_shared = shared.clone();
        let wrapper_callback = move |data: JsValue| {
            {
                let mut guard = c_shared.state.lock().unwrap();
                match &*guard {
                    ThreadState::Initialized => {}
                    _ => panic!("Illegal thread state!"),
                };
                *guard = ThreadState::Started;
            }
            Box::pin(async move {
                let result: T = f(data).await;
                {
                    let mut guard = c_shared.state.lock().unwrap();
                    *guard = ThreadState::Finished(result);
                }
                c_shared.cond_var.notify_all();
            }) as Pin<Box<dyn Future<Output = ()>>>
        };

        let boxed: Box<dyn FnOnce(JsValue) -> Pin<Box<dyn Future<Output = ()>>> + Send + 'static> =
            Box::new(wrapper_callback);
        let double_boxed = Box::new(boxed);
        // Double Pointer to turn a fat pointer (Box<dyn Fn>) into a regular pointer.
        let double_boxed_ptr = Box::into_raw(double_boxed);
        let ptr_number: usize = unsafe { std::mem::transmute(double_boxed_ptr) };
        // wasm_bindgen doesn't support Boxed FnOnce with Future so we have to resort to hacks.

        start_thread_worker(
            wasm_bindgen::module().dyn_into().unwrap(),
            wasm_bindgen::memory().dyn_into().unwrap(),
            ptr_number,
            data,
            name.unwrap_or("Thread"),
        );

        log::info!("Started WASM thread");
        JoinHandle(shared)
    }
}

enum ThreadState<T: Send> {
    Initialized,
    Started,
    Finished(T),
    Joined,
}

pub struct ThreadShared<T: Send> {
    state: Mutex<ThreadState<T>>,
    cond_var: Condvar,
}

#[wasm_bindgen(raw_module = "../../www/src/web_glue.ts")]
extern "C" {
    #[wasm_bindgen(js_name = "startThreadWorker")]
    fn start_thread_worker(
        module: WebAssembly::Module,
        memory: WebAssembly::Memory,
        callback_ptr: usize, // Wasm32 -> u32 Pointer. And it avoids a BigInt in JS
        data: JsValue,
        name: &str,
    );
}

#[wasm_bindgen(js_name = "threadFunc")]
pub async fn thread_func(callback_ptr: usize, data: JsValue) {
    let callback_ptr: *mut Box<
        dyn FnOnce(JsValue) -> Pin<Box<dyn Future<Output = ()>>> + Send + 'static,
    > = std::ptr::without_provenance_mut(callback_ptr);
    let callback = unsafe { Box::from_raw(callback_ptr) };
    callback(data).await;
}
