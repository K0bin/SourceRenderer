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
                let buffer = if info.usage.contains(gpu::BufferUsage::ACCELERATION_STRUCTURE) {
                    let heap_descriptor = metal::HeapDescriptor::new();
                    options |= metal::MTLResourceOptions::HazardTrackingModeTracked;
                    let size = device.heap_buffer_size_and_align(info.size, options);
                    heap_descriptor.set_size(size.size);
                    let heap = device.new_heap(&heap_descriptor);
                    let buffer = heap.new_buffer_with_offset(info.size, options, 0u64).unwrap();
                    buffer.make_aliasable();
                    buffer
                } else {
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
                };
                buffer
            },
            ResourceMemory::Suballocated { memory, offset } => {
                options |= metal::MTLResourceOptions::HazardTrackingModeUntracked;
                options |= memory.resource_options();
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

    unsafe fn map(&self, offset: u64, _length: u64, _invalidate: bool) -> Option<*mut c_void> {
        let ptr = self.buffer.contents();
        if ptr == std::ptr::null_mut() {
            return None;
        }
        return Some(ptr.offset(offset as isize));
    }

    unsafe fn unmap(&self, _offset: u64, _length: u64, _flush: bool) {
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
