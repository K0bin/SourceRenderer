use js_sys::wasm_bindgen::JsValue;
use web_sys::{Gpu, OffscreenCanvas};

use sourcerenderer_core::gpu;

use crate::{WebGPUBackend, WebGPUDevice, WebGPUInstance, WebGPUSwapchain};

#[derive(PartialEq, Debug)]
enum WebGPUSurfaceCanvas {
    Fake,
    Canvas(OffscreenCanvas)
}

pub struct WebGPUSurface {
    instance: Gpu,
    canvas: WebGPUSurfaceCanvas,
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
            canvas: WebGPUSurfaceCanvas::Canvas(canvas),
        })
    }

    pub fn new_fake(instance: &WebGPUInstance) -> Result<Self, ()> {
        Ok(Self {
            instance: instance.handle().clone(),
            canvas: WebGPUSurfaceCanvas::Fake,
        })
    }

    #[inline(always)]
    pub(crate) fn canvas(&self) -> &OffscreenCanvas {
        if let WebGPUSurfaceCanvas::Canvas(canvas) = &self.canvas {
            canvas
        } else {
            panic!("Surface only has a fake canvas.")
        }
    }

    #[inline(always)]
    pub(crate) fn instance_handle(&self) -> &Gpu {
        &self.instance
    }

    #[inline(always)]
    pub fn take_canvas(self) -> OffscreenCanvas {
        if let WebGPUSurfaceCanvas::Canvas(canvas) = self.canvas {
            canvas
        } else {
            panic!("Surface only has a fake canvas.")
        }
    }


    #[inline(always)]
    pub fn take_js_val(self) -> JsValue {
        if let WebGPUSurfaceCanvas::Canvas(canvas) = self.canvas {
            canvas.into()
        } else {
            JsValue::from_str("FAKE_CANVAS")
        }
    }

    #[inline(always)]
    pub fn take_fake_canvas(self) -> JsValue {
        assert_eq!(self.canvas, WebGPUSurfaceCanvas::Fake);
        JsValue::from_str("FAKE_CANVAS")
    }
}

impl gpu::Surface<WebGPUBackend> for WebGPUSurface {
    unsafe fn create_swapchain(self, _width: u32, _height: u32, _vsync: bool, device: &WebGPUDevice) -> Result<WebGPUSwapchain, gpu::SwapchainError> {
        Ok(WebGPUSwapchain::new(device.handle(), self))
    }
}
