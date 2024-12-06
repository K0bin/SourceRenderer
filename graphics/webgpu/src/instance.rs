use sourcerenderer_core::gpu::Instance;
use web_sys::{wasm_bindgen, GpuAdapter, Navigator};
use wasm_bindgen_futures::*;

use crate::WebGPUBackend;

pub struct WebGPUInstance {

}

impl WebGPUInstance {
    pub async fn new(navigator: Navigator) -> Result<WebGPUInstance, ()> {
        let gpu = navigator.gpu();
        if !gpu.is_object() {
            panic!("Browser does not support WebGPU");
        }
        let adapter_future = JsFuture::from(gpu.request_adapter());
        let adapter: GpuAdapter = adapter_future
            .await
            .map_err(|_| ())?
            .into();

        unimplemented!()
    }
}

/*impl Instance<WebGPUBackend> for WebGPUInstance {
    fn list_adapters(&self) -> &[<WebGPUBackend as sourcerenderer_core::gpu::GPUBackend>::Adapter] {
        todo!()
    }
}*/
