use sourcerenderer_core::{atomic_refcell::{AtomicRef, AtomicRefCell}, gpu};
use web_sys::{GpuDevice, GpuQuerySet, GpuQueryType, GpuQuerySetDescriptor};

pub struct WebGPUQueryPool {
    device: GpuDevice,
    descriptor: GpuQuerySetDescriptor,
    query_set: AtomicRefCell<GpuQuerySet>,
}

impl WebGPUQueryPool {
    pub(crate) fn new(device: &GpuDevice, count: u32) -> Self {
        let descriptor = GpuQuerySetDescriptor::new(count, GpuQueryType::Occlusion);
        let query_set = device.create_query_set(&descriptor).unwrap();
        Self {
            device: device.clone(),
            descriptor,
            query_set: AtomicRefCell::new(query_set)
        }
    }

    pub(crate) fn handle(&self) -> AtomicRef<'_, GpuQuerySet> {
        self.query_set.borrow()
    }
}

impl gpu::QueryPool for WebGPUQueryPool {
    unsafe fn reset(&self) {
        let mut query_set = self.query_set.borrow_mut();
        *query_set = self.device.create_query_set(&self.descriptor).unwrap()
    }
}
