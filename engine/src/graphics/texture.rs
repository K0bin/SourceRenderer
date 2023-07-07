use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::gpu::*;

use super::*;

pub struct GPUTexture<B: GPUBackend> {
    device: Arc<B::Device>,
    texture: ManuallyDrop<B::Texture>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for GPUTexture<B> {
    fn drop(&mut self) {
        let texture = unsafe { ManuallyDrop::take(&mut self.texture) };
        self.destroyer.destroy_texture(texture);
    }
}

impl<B: GPUBackend> GPUTexture<B> {
    pub(crate) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, info: &TextureInfo, name: Option<&str>) -> Self {
        let texture = unsafe { device.create_texture(info, name) };
        Self {
            device: device.clone(),
            texture: ManuallyDrop::new(texture),
            destroyer: destroyer.clone()
        }
    }

    pub(crate) fn handle(&self) -> &B::Texture {
        &self.texture
    }
}

pub struct GPUTextureView<B: GPUBackend> {
    device: Arc<B::Device>,
    texture: Arc<GPUTexture<B>>,
    texture_view: ManuallyDrop<B::TextureView>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for GPUTextureView<B> {
    fn drop(&mut self) {
        let texture_view = unsafe { ManuallyDrop::take(&mut self.texture_view) };
        self.destroyer.destroy_texture_view(texture_view);
    }
}

impl<B: GPUBackend> GPUTextureView<B> {
    pub(crate) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, texture: &Arc<GPUTexture<B>>, info: &TextureViewInfo, name: Option<&str>) -> Self {
        let texture_view = unsafe { device.create_texture_view(texture.handle(), info, name) };
        Self {
            device: device.clone(),
            texture: texture.clone(),
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone()
        }
    }
}
