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
    pub fn create_context(&self) -> GraphicsContext<B> {
        assert!(!self.has_context.swap(true, Ordering::AcqRel));
        GraphicsContext::new(&self.device, &self.destroyer, self.prerendered_frames)
    }

    pub fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> super::Texture<B> {
        super::Texture::new(&self.device, &self.destroyer, info, name)
    }
}
