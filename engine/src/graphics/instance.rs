use smallvec::SmallVec;
use sourcerenderer_core::gpu::{GPUBackend, Instance as GPUInstance, AdapterType, Adapter as GPUAdapter};

pub struct Instance<B: GPUBackend> {
    instance: B::Instance,
    adapters: SmallVec<[Adapter<B>; 2]>
}

impl<B: GPUBackend> Instance<B> {
    pub fn new(instance: B::Instance) -> Self {
        let adapters: SmallVec<[Adapter<B>; 2]> = instance.list_adapters()
            .iter()
            .map(|a| Adapter {
                adapter: a as *const B::Adapter
            })
            .collect();

        Self {
            instance,
            adapters
        }
    }

    pub fn list_adapters(&self) -> &[Adapter<B>] {
        &self.adapters
    }
}

pub struct Adapter<B: GPUBackend> {
    adapter: *const B::Adapter
}

impl<B: GPUBackend> Adapter<B> {
    pub fn adapter_type(&self) -> AdapterType {
        unsafe { (*self.adapter).adapter_type() }
    }

    pub fn create_device(&self, surface: &B::Surface) -> super::Device<B> {
        let device = unsafe { (*self.adapter).create_device(surface) };
        super::Device::new(device)
    }
}
