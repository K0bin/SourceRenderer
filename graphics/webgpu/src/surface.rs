use web_sys::{Gpu, OffscreenCanvas};

use crate::WebGPUInstance;

pub struct WebGPUSurface {
    instance: Gpu,
    canvas: OffscreenCanvas
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
            canvas
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
}
