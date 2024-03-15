use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{GPUBackend, Instance as GPUInstance, AdapterType, Adapter as GPUAdapter};

pub struct Instance<B: GPUBackend> {
    instance: Arc<B::Instance>,
    adapters: SmallVec<[Adapter<B>; 2]>
}

impl<B: GPUBackend> Instance<B> {
    pub fn new(instance: B::Instance) -> Arc<Self> {
        let instance_arc = Arc::new(instance);

        let adapters: SmallVec<[Adapter<B>; 2]> = instance_arc.list_adapters()
            .iter()
            .map(|a| Adapter {
                adapter: a as *const B::Adapter,
                instance: instance_arc.clone()
            })
            .collect();

        let result = Arc::new(Self {
            instance: instance_arc,
            adapters
        });

        result
    }

    pub fn list_adapters(&self) -> &[Adapter<B>] {
        &self.adapters
    }

    pub fn handle(&self) -> &B::Instance {
        &self.instance
    }
}

pub struct Adapter<B: GPUBackend> {
    adapter: *const B::Adapter,
    instance: Arc<B::Instance>
}

impl<B: GPUBackend> Adapter<B> {
    pub fn adapter_type(&self) -> AdapterType {
        unsafe { (*self.adapter).adapter_type() }
    }

    pub fn create_device(&self, surface: &B::Surface) -> Arc<super::Device<B>> {
        let device = unsafe { (*self.adapter).create_device(surface) };
        Arc::new(super::Device::new(&self.instance, device))
    }
}

unsafe impl<B: GPUBackend> Sync for Adapter<B> {}
unsafe impl<B: GPUBackend> Send for Adapter<B> {}
