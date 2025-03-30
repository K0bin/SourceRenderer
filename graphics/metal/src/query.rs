use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSUInteger;
use objc2_metal::{self, MTLBuffer as _, MTLDevice as _};
use sourcerenderer_core::gpu::{self, QueryPool};

pub struct MTLQueryPool {
    buffer: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    count: u32,
}

unsafe impl Send for MTLQueryPool {}
unsafe impl Sync for MTLQueryPool {}

impl MTLQueryPool {
    pub(crate) fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, count: u32) -> Self {
        let buffer = device.newBufferWithLength_options((count as NSUInteger) * std::mem::size_of::<u64>(),
            objc2_metal::MTLResourceOptions::StorageModeShared).unwrap();

        let result = Self {
            buffer,
            count,
        };

        unsafe {
            result.reset();
        }

        result
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLBuffer> {
        self.buffer.as_ref()
    }
}

impl gpu::QueryPool for MTLQueryPool {
    unsafe fn reset(&self) {
        let ptr = self.buffer.contents();
        let slice: &mut [u64] = std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u64, self.count as usize);
        slice.fill(0u64);
    }
}
