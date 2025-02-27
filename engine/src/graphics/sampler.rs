use std::{sync::Arc, mem::ManuallyDrop};

use sourcerenderer_core::gpu::*;

use super::*;

pub struct Sampler<B: GPUBackend> {
    sampler: ManuallyDrop<B::Sampler>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Sampler<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, info: &SamplerInfo) -> Self {
        let sampler = unsafe { device.create_sampler(info) };
        Self {
            sampler: ManuallyDrop::new(sampler),
            destroyer: destroyer.clone()
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &B::Sampler {
        &self.sampler
    }
}

impl<B: GPUBackend> Drop for Sampler<B> {
    fn drop(&mut self) {
        let sampler = unsafe { ManuallyDrop::take(&mut self.sampler) };
        self.destroyer.destroy_sampler(sampler);
    }
}
