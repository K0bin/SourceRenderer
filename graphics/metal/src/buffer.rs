use std::{ffi::c_void, hash::Hash};

use metal;
use metal::foreign_types::ForeignType;

use sourcerenderer_core::gpu;

use crate::heap::ResourceMemory;

pub struct MTLBuffer {
    info: gpu::BufferInfo,
    buffer: metal::Buffer
}

impl MTLBuffer {
    pub(crate) fn new(memory: ResourceMemory, info: &gpu::BufferInfo, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let mut options = Self::resource_options(info);
        let buffer = match memory {
            ResourceMemory::Dedicated { device, options: memory_options } => {
                if info.usage.gpu_writable() {
                    options |= metal::MTLResourceOptions::HazardTrackingModeTracked;
                } else {
                    options |= metal::MTLResourceOptions::HazardTrackingModeUntracked;
                }
                let buffer = device.new_buffer(info.size, options | memory_options);
                if buffer.as_ptr() == std::ptr::null_mut() {
                    return Err(gpu::OutOfMemoryError {});
                }
                buffer
            },
            ResourceMemory::Suballocated { memory, offset } => {
                options |= metal::MTLResourceOptions::HazardTrackingModeUntracked;
                let buffer_opt = memory.handle().new_buffer_with_offset(info.size, options, offset);
                if buffer_opt.is_none() {
                    return Err(gpu::OutOfMemoryError {});
                }
                buffer_opt.unwrap()
            }
        };
        if let Some(name) = name {
            buffer.add_debug_marker(name, metal::NSRange {
                location: 0u64,
                length: info.size
            });
        }
        Ok(Self {
            info: info.clone(),
            buffer
        })
    }

    pub(crate) fn resource_options(_info: &gpu::BufferInfo) -> metal::MTLResourceOptions {
        let options = metal::MTLResourceOptions::empty();
        options
    }

    pub(crate) fn handle(&self) -> &metal::BufferRef {
        &self.buffer
    }
}

impl gpu::Buffer for MTLBuffer {
    fn info(&self) -> &gpu::BufferInfo {
        &self.info
    }

    unsafe fn map(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void> {
        let ptr = self.buffer.contents();
        if ptr == std::ptr::null_mut() {
            return None;
        }
        return Some(ptr);
    }

    unsafe fn unmap(&self, offset: u64, length: u64, flush: bool) {
        if flush {
            self.buffer.did_modify_range(metal::NSRange {
                location: offset,
                length
            });
        }
    }
}

impl Hash for MTLBuffer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.buffer.as_ptr().hash(state);
    }
}

impl PartialEq<MTLBuffer> for MTLBuffer {
    fn eq(&self, other: &MTLBuffer) -> bool {
        self.buffer.as_ptr() == other.buffer.as_ptr()
    }
}

impl Eq for MTLBuffer {}
