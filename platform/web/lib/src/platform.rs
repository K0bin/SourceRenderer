use sourcerenderer_core::{platform::ThreadHandle, Platform};
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUInstance, WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError};
use web_sys::{Navigator, OffscreenCanvas};

use crate::{io::WebIO, window::WebWindow};

pub struct WebPlatform {
    window: WebWindow,
    instance_init: Result<WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError>
}

impl WebPlatform {
    pub(crate) async fn new(navigator: Navigator, canvas: OffscreenCanvas) -> Self {
        let window = WebWindow::new(canvas);
        let instance_init = WebGPUInstance::async_init(navigator).await;
        Self {
            window,
            instance_init
        }
    }
}

impl Platform for WebPlatform {
    type GPUBackend = WebGPUBackend;
    type Window = WebWindow;
    type IO = WebIO;
    type ThreadHandle = NoThreadsThreadHandle;

    fn window(&self) -> &WebWindow {
        &self.window
    }

    fn create_graphics(&self, _debug_layers: bool) -> Result<WebGPUInstance, Box<dyn std::error::Error>> {
        self.instance_init.as_ref()
            .map(|init| {
            WebGPUInstance::new(init)
            })
            .map_err(|e| Box::new(e.clone()) as Box<dyn std::error::Error>)
    }

    fn thread_memory_management_pool<F, T>(callback: F) -> T
        where F: FnOnce() -> T {
        callback()
    }
}

pub struct NoThreadsThreadHandle {}
impl ThreadHandle for NoThreadsThreadHandle {
    fn join(self) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {Ok(())}
}
