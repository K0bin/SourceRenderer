use std::sync::{Arc, Weak};

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{Instance as _, Adapter as _};

use super::*;

pub struct Instance {
    instance: Arc<active_gpu_backend::Instance>,
    adapters: SmallVec<[Adapter; 2]>
}

impl Instance {
    pub fn new(instance: active_gpu_backend::Instance) -> Arc<Self> {
        let instance_arc = Arc::new(instance);

        let result: Arc<Self> = Arc::new_cyclic(|result_weak| {
            Self {
                instance: instance_arc.clone(),
                adapters: instance_arc.list_adapters()
                .iter()
                .map(|a| Adapter {
                    adapter: a as *const active_gpu_backend::Adapter,
                    instance: result_weak.clone()
                })
                .collect()
            }
        });

        result
    }

    #[inline(always)]
    pub fn list_adapters(&self) -> &[Adapter] {
        &self.adapters
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::Instance {
        &self.instance
    }
}

pub struct Adapter {
    adapter: *const active_gpu_backend::Adapter,
    instance: Weak<Instance>
}

impl Adapter {
    #[inline(always)]
    pub fn adapter_type(&self) -> AdapterType {
        unsafe { (*self.adapter).adapter_type() }
    }

    pub fn create_device(&self, surface: &active_gpu_backend::Surface) -> Arc<super::Device> {
        let device = unsafe { (*self.adapter).create_device(surface) };
        let instance = self.instance.upgrade().unwrap();
        Arc::new(super::Device::new(device, instance))
    }
}

unsafe impl Sync for Adapter {}
unsafe impl Send for Adapter {}
