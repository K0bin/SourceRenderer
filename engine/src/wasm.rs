use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::future::Future;

use wasm_bindgen::prelude::{wasm_bindgen, JsValue, JsCast as _};
use wasm_bindgen::closure::Closure;
use js_sys::WebAssembly;

// Wasm thread
pub mod thread {

    use super::*;

    pub struct JoinHandle<T: Send>(Arc<ThreadShared<T>>);
    impl<T: Send> JoinHandle<T> {
        pub fn join(self) -> T {
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
                data
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
        T: Send + 'static {
        //spawn_with_js_val(|_| f(), JsValue::null())
        unimplemented!()
    }

    pub fn spawn_with_js_val<F, TF, T>(f: F, data: JsValue) -> JoinHandle<T>
    where
        F: FnOnce(JsValue) -> TF + Send + 'static,
        TF: Future<Output = T> + 'static,
        T: Send + Unpin + 'static {

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
                    ThreadState::Initialized => {},
                    _ => panic!("Illegal thread state!")
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

        let boxed: Box<dyn FnOnce(JsValue) -> Pin<Box<dyn Future<Output = ()>>> + Send + 'static> = Box::new(wrapper_callback);
        let boxed_ptr = Box::into_raw(boxed);
        let boxed_ptr_workaround: u64 = unsafe { std::mem::transmute(boxed_ptr) };
        // wasm_bindgen doesn't support FnOnce so we have to resort to hacks.

        unsafe {
            start_thread_worker(
                wasm_bindgen::module().dyn_into().unwrap(),
                wasm_bindgen::memory().dyn_into().unwrap(),
                boxed_ptr_workaround,
                data,
            );
        }

        log::info!("Started WASM thread");
        JoinHandle(shared)
    }
}

enum ThreadState<T: Send> {
    Initialized,
    Started,
    Finished(T),
    Joined
}

pub struct ThreadShared<T: Send> {
    state: Mutex<ThreadState<T>>,
    cond_var: Condvar,
}


#[wasm_bindgen(raw_module = "../../www/src/web_glue.ts")] // (module = "/src/web_glue/web_glue.ts")]
extern "C" {
    #[wasm_bindgen(js_name = "startThreadWorker")]
    fn start_thread_worker(
        module: WebAssembly::Module,
        memory: WebAssembly::Memory,
        callback_ptr: u64, // dyn => fat pointer => Pointer size of wasm32 is 32 => u64
        data: JsValue,
    );
}

#[wasm_bindgen(js_name = "threadFunc")]
pub async fn thread_func(
    callback_ptr: u64,
    data: JsValue,
) {
    let callback_ptr: *mut (dyn FnOnce(JsValue) -> Pin<Box<dyn Future<Output = ()>>> + Send + 'static) = unsafe { std::mem::transmute(callback_ptr) };
    let callback = unsafe { Box::from_raw(callback_ptr) };
    callback(data).await;
}
