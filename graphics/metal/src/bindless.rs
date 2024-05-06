use metal;
use smallvec::{smallvec, SmallVec};

use sourcerenderer_core::gpu;

use super::*;

pub(crate) struct MTLBindlessArgumentBuffer {
    argument_buffer: metal::Buffer
}

impl MTLBindlessArgumentBuffer {
    pub(crate) fn new(device: &metal::DeviceRef, size: usize) -> Self {
        let buffer = device.new_buffer((std::mem::size_of::<metal::MTLResourceID>() * size) as u64, metal::MTLResourceOptions::StorageModeShared);
        Self {
            argument_buffer: buffer
        }
    }

    pub(crate) fn insert(&self, texture_view: &MTLTextureView, slot: u32) {
        unsafe {
            let ptr = self.argument_buffer.contents();
            let mut resource_id_ptr: *mut metal::MTLResourceID = std::mem::transmute(ptr);
            resource_id_ptr = resource_id_ptr.offset(slot as isize);
            *resource_id_ptr = texture_view.handle().gpu_resource_id();
        }
    }

    pub(crate) fn handle(&self) -> &metal::BufferRef {
        &self.argument_buffer
    }
}
