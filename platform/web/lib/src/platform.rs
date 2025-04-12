use sourcerenderer_core::{atomic_refcell::AtomicRefCell, platform::GraphicsPlatform};
use sourcerenderer_webgpu::{WebGPUBackend, WebGPUInstance, WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError};
use web_sys::{Navigator, OffscreenCanvas};

use crate::window::WebWindow;

thread_local! {
    static GPU_INIT: AtomicRefCell<Result<WebGPUInstanceAsyncInitResult, WebGPUInstanceInitError>> = AtomicRefCell::new(Err(WebGPUInstanceInitError::uninited()));
}

pub struct WebPlatform {
    window: WebWindow,
}

impl WebPlatform {
    pub(crate) async fn new(navigator: Navigator, canvas: OffscreenCanvas) -> Self {
        let window = WebWindow::new(canvas);
        init_webgpu_on_thread(navigator).await;
        Self {
            window,
        }
    }

    pub(crate) fn window(&self) -> &WebWindow {
        &self.window
    }
}

impl GraphicsPlatform<WebGPUBackend> for WebPlatform {
    fn create_instance(debug_layers: bool) -> Result<<WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Instance, Box<dyn std::error::Error>> {
        GPU_INIT.with(|gpu_init_refcell| {
            let gpu_init = gpu_init_refcell.borrow();
            gpu_init.as_ref().map(|init| {
                    WebGPUInstance::new(init, debug_layers)
                })
                .map_err(|e| Box::new(e.clone()) as Box<dyn std::error::Error>)
        })
    }
}

async fn init_webgpu_on_thread(navigator: Navigator) {
    GPU_INIT.with(|gpu_init_refcell| {
        let mut gpu_init = gpu_init_refcell.borrow_mut();
        *gpu_init = Err(WebGPUInstanceInitError::unfinished());
    });
    let instance_init = WebGPUInstance::async_init(navigator).await;
    GPU_INIT.with(|gpu_init_refcell| {
        let mut gpu_init = gpu_init_refcell.borrow_mut();
        *gpu_init = instance_init;
    });
}
