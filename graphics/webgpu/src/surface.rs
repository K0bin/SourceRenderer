use js_sys::wasm_bindgen::JsValue;
use std::marker::PhantomData;
use web_sys::{Gpu, GpuCanvasContext, HtmlCanvasElement, OffscreenCanvas};

use sourcerenderer_core::gpu;

use crate::{WebGPUBackend, WebGPUDevice, WebGPUInstance, WebGPUSwapchain};

#[derive(PartialEq)]
enum CanvasKind {
    Offscreen(OffscreenCanvas),
    Dom(HtmlCanvasElement),
}

pub struct WebGPUSurface {
    canvas: CanvasKind,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl PartialEq for WebGPUSurface {
    fn eq(&self, other: &Self) -> bool {
        self.canvas == other.canvas
    }
}

impl Eq for WebGPUSurface {}

impl WebGPUSurface {
    pub fn new_offscreen(canvas: OffscreenCanvas) -> Result<Self, ()> {
        Ok(Self {
            canvas: CanvasKind::Offscreen(canvas),
            _p: PhantomData,
        })
    }

    pub fn new_dom(canvas: HtmlCanvasElement) -> Result<Self, ()> {
        Ok(Self {
            canvas: CanvasKind::Dom(canvas),
            _p: PhantomData,
        })
    }

    pub(crate) fn context(&self) -> GpuCanvasContext {
        let context_opt = match &self.canvas {
            CanvasKind::Offscreen(c) => c.get_context("webgpu"),
            CanvasKind::Dom(c) => c.get_context("webgpu"),
        };

        let context_obj: JsValue = context_opt
            .expect("Failed to retrieve context from OffscreenCanvas")
            .expect("Failed to retrieve context from OffscreenCanvas")
            .into();
        context_obj.into()
    }

    pub(crate) fn width(&self) -> u32 {
        match &self.canvas {
            CanvasKind::Offscreen(c) => c.width(),
            CanvasKind::Dom(c) => c.width(),
        }
    }

    pub(crate) fn height(&self) -> u32 {
        match &self.canvas {
            CanvasKind::Offscreen(c) => c.height(),
            CanvasKind::Dom(c) => c.height(),
        }
    }

    #[inline(always)]
    pub fn transfer_canvas(self) -> OffscreenCanvas {
        match self.canvas {
            CanvasKind::Offscreen(c) => c,
            CanvasKind::Dom(c) => c.transfer_control_to_offscreen().unwrap(),
        }
    }
}

impl gpu::Surface<WebGPUBackend> for WebGPUSurface {
    unsafe fn create_swapchain(
        self,
        width: u32,
        height: u32,
        _vsync: bool,
        device: &WebGPUDevice,
    ) -> Result<WebGPUSwapchain, gpu::SwapchainError> {
        Ok(WebGPUSwapchain::new(device.handle(), self, width, height))
    }
}
