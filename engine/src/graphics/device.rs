use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use sourcerenderer_core::gpu::*;

use super::*;

struct GPUDevice<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    prerendered_frames: u32,
    has_context: AtomicBool
}

impl<B: GPUBackend> GPUDevice<B> {
    pub fn create_context(&self) -> GPUContext<B> {
        assert!(!self.has_context.swap(true, Ordering::AcqRel));
        GPUContext::new(&self.device, &self.destroyer, self.prerendered_frames)
    }

    pub fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> GPUTexture<B> {
        GPUTexture::new(&self.device, &self.destroyer, info, name)
    }
}
