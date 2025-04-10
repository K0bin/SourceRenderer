use std::{sync::Arc, mem::ManuallyDrop};

use super::*;

pub struct Sampler {
    sampler: ManuallyDrop<active_gpu_backend::Sampler>,
    destroyer: Arc<DeferredDestroyer>
}

impl Sampler {
    pub(super) fn new(device: &Arc<active_gpu_backend::Device>, destroyer: &Arc<DeferredDestroyer>, info: &SamplerInfo) -> Self {
        let sampler = unsafe { device.create_sampler(info) };
        Self {
            sampler: ManuallyDrop::new(sampler),
            destroyer: destroyer.clone()
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::Sampler {
        &self.sampler
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        let sampler = unsafe { ManuallyDrop::take(&mut self.sampler) };
        self.destroyer.destroy_sampler(sampler);
    }
}
