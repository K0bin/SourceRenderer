use std::sync::{Arc, Weak};

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{Instance as _, Adapter as _};

use super::*;

pub struct Instance<B: GPUBackend> {
    instance: Arc<B::Instance>,
    adapters: SmallVec<[Adapter<B>; 2]>
}

impl<B: GPUBackend> Instance<B> {
    pub fn new(instance: B::Instance) -> Arc<Self> {
        let instance_arc = Arc::new(instance);

        let result = Arc::new_cyclic(|result_weak| {
            Self {
                instance: instance_arc.clone(),
                adapters: instance_arc.list_adapters()
                .iter()
                .map(|a| Adapter {
                    adapter: a as *const B::Adapter,
                    instance: result_weak.clone()
                })
                .collect()
            }
        });

        result
    }

    #[inline(always)]
    pub fn list_adapters(&self) -> &[Adapter<B>] {
        &self.adapters
    }

    #[inline(always)]
    pub fn handle(&self) -> &B::Instance {
        &self.instance
    }
}

pub struct Adapter<B: GPUBackend> {
    adapter: *const B::Adapter,
    instance: Weak<Instance<B>>
}

impl<B: GPUBackend> Adapter<B> {
    #[inline(always)]
    pub fn adapter_type(&self) -> AdapterType {
        unsafe { (*self.adapter).adapter_type() }
    }

    pub fn create_device(&self, surface: &B::Surface) -> Arc<super::Device<B>> {
        let device = unsafe { (*self.adapter).create_device(surface) };
        let instance = self.instance.upgrade().unwrap();
        Arc::new(super::Device::new(device, instance))
    }
}

unsafe impl<B: GPUBackend> Sync for Adapter<B> {}
unsafe impl<B: GPUBackend> Send for Adapter<B> {}
