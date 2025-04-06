use sourcerenderer_core::{platform::{GraphicsPlatform, WindowProvider}, Platform};
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
    type IO = WebIO;
}

impl GraphicsPlatform<WebGPUBackend> for WebPlatform {
    fn create_instance(&self, debug_layers: bool) -> Result<<WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Instance, Box<dyn std::error::Error>> {
        self.instance_init.as_ref()
            .map(|init| {
            WebGPUInstance::new(init, debug_layers)
            })
            .map_err(|e| Box::new(e.clone()) as Box<dyn std::error::Error>)
    }
}

impl WindowProvider<WebGPUBackend> for WebPlatform {
    type Window = WebWindow;

    fn window(&self) -> &Self::Window {
        &self.window
    }
}
