use sourcerenderer_core::platform::GraphicsPlatform;
use sourcerenderer_webgpu::{
    WebGPUBackend, WebGPUInstance,
};
use web_sys::OffscreenCanvas;

use crate::window::WebWindow;

pub struct WebPlatform {
    window: WebWindow,
}

impl WebPlatform {
    pub(crate) async fn new_on_worker(canvas: OffscreenCanvas) -> Self {
        let window = WebWindow::new(canvas);
        Self { window }
    }

    pub(crate) fn window(&self) -> &WebWindow {
        &self.window
    }
}

impl GraphicsPlatform<WebGPUBackend> for WebPlatform {
    fn create_instance(
        debug_layers: bool,
    ) -> Result<
        <WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Instance,
        Box<dyn std::error::Error>,
    > {
        Ok(WebGPUInstance::new(debug_layers))
    }
}
