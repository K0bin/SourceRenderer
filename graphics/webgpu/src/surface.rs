use js_sys::wasm_bindgen::JsValue;
use std::marker::PhantomData;
use web_sys::{Gpu, OffscreenCanvas};

use sourcerenderer_core::gpu;

use crate::{WebGPUBackend, WebGPUDevice, WebGPUInstance, WebGPUSwapchain};

pub struct WebGPUSurface {
    instance: Gpu,
    canvas: OffscreenCanvas,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl PartialEq for WebGPUSurface {
    fn eq(&self, other: &Self) -> bool {
        self.canvas == other.canvas
    }
}

impl Eq for WebGPUSurface {}

impl WebGPUSurface {
    pub fn new(instance: &WebGPUInstance, canvas: OffscreenCanvas) -> Result<Self, ()> {
        Ok(Self {
            instance: instance.handle().clone(),
            canvas,
            _p: PhantomData,
        })
    }

    #[inline(always)]
    pub(crate) fn canvas(&self) -> &OffscreenCanvas {
        &self.canvas
    }

    #[inline(always)]
    pub(crate) fn instance_handle(&self) -> &Gpu {
        &self.instance
    }

    #[inline(always)]
    pub fn take_canvas(self) -> OffscreenCanvas {
        self.canvas
    }
}

impl gpu::Surface<WebGPUBackend> for WebGPUSurface {
    unsafe fn create_swapchain(
        self,
        _width: u32,
        _height: u32,
        _vsync: bool,
        device: &WebGPUDevice,
    ) -> Result<WebGPUSwapchain, gpu::SwapchainError> {
        Ok(WebGPUSwapchain::new(device.handle(), self))
    }
}
