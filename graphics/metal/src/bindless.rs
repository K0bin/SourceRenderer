use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSString, NSUInteger};
use objc2_metal::{self, MTLBuffer as _, MTLDevice, MTLResource, MTLTexture};

use super::*;

pub(crate) struct MTLBindlessArgumentBuffer {
    argument_buffer: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
}

impl MTLBindlessArgumentBuffer {
    pub(crate) unsafe fn new(
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        size: usize,
    ) -> Self {
        let buffer = device
            .newBufferWithLength_options(
                (std::mem::size_of::<objc2_metal::MTLResourceID>() * size) as NSUInteger,
                objc2_metal::MTLResourceOptions::StorageModeShared,
            )
            .unwrap();
        buffer.setLabel(Some(&NSString::from_str("Bindless textures")));
        Self {
            argument_buffer: buffer,
        }
    }

    pub(crate) fn insert(&self, texture_view: &MTLTextureView, slot: u32) {
        unsafe {
            let ptr = self.argument_buffer.contents();
            let mut resource_id_ptr: *mut objc2_metal::MTLResourceID = std::mem::transmute(ptr);
            resource_id_ptr = resource_id_ptr.offset(slot as isize);
            *resource_id_ptr = texture_view.handle().gpuResourceID();
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLBuffer> {
        &self.argument_buffer
    }
}
