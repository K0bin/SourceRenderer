use std::{ffi::c_void, hash::Hash};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSUInteger;
use objc2_metal::{self, MTLBuffer as _, MTLDevice as _, MTLHeap as _, MTLResource as _};

use sourcerenderer_core::gpu;

use crate::heap::ResourceMemory;

pub struct MTLBuffer {
    info: gpu::BufferInfo,
    buffer: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    _heap: Option<Retained<ProtocolObject<dyn objc2_metal::MTLHeap>>>
}

unsafe impl Send for MTLBuffer {}
unsafe impl Sync for MTLBuffer {}

impl MTLBuffer {
    pub(crate) unsafe fn new(memory: ResourceMemory, info: &gpu::BufferInfo, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let mut options = Self::resource_options(info);
        let (buffer, heap) = match memory {
            ResourceMemory::Dedicated { device, options: memory_options } => {
                let buffer = if info.usage.contains(gpu::BufferUsage::ACCELERATION_STRUCTURE) {
                    let heap_descriptor = objc2_metal::MTLHeapDescriptor::new();
                    options |= objc2_metal::MTLResourceOptions::HazardTrackingModeTracked;
                    let size = device.heapBufferSizeAndAlignWithLength_options(info.size as NSUInteger, options);
                    heap_descriptor.setSize(size.size);
                    if !memory_options.contains(objc2_metal::MTLResourceOptions::StorageModePrivate) {
                        panic!("Acceleration structure memory must not be cpu accessible");
                    }
                    heap_descriptor.setType(objc2_metal::MTLHeapType::Placement);
                    heap_descriptor.setResourceOptions(options | memory_options);
                    let heap_opt = device.newHeapWithDescriptor(&heap_descriptor);
                    if heap_opt.is_none() {
                        return Err(gpu::OutOfMemoryError {});
                    }
                    let heap = heap_opt.unwrap();
                    let buffer = heap.newBufferWithLength_options(info.size as NSUInteger, options | memory_options).unwrap();
                    buffer.makeAliasable();
                    (buffer, Some(heap))
                } else {
                    if info.usage.gpu_writable() {
                        options |= objc2_metal::MTLResourceOptions::HazardTrackingModeTracked;
                    } else {
                        options |= objc2_metal::MTLResourceOptions::HazardTrackingModeUntracked;
                    }
                    let buffer_opt = device.newBufferWithLength_options(info.size as NSUInteger, options | memory_options);
                    if buffer_opt.is_none() {
                        return Err(gpu::OutOfMemoryError {});
                    }
                    (buffer_opt.unwrap(), None)
                };
                buffer
            },
            ResourceMemory::Suballocated { memory, offset } => {
                options |= objc2_metal::MTLResourceOptions::HazardTrackingModeUntracked;
                options |= memory.resource_options();
                let buffer_opt = memory.handle().newBufferWithLength_options_offset(info.size as NSUInteger, options, offset as NSUInteger);
                if buffer_opt.is_none() {
                    return Err(gpu::OutOfMemoryError {});
                }
                (buffer_opt.unwrap(), None)
            }
        };
        if let Some(name) = name {
            buffer.addDebugMarker_range(&objc2_foundation::NSString::from_str(name), objc2_foundation::NSRange {
                location: 0,
                length: info.size as NSUInteger
            });
        }
        Ok(Self {
            info: info.clone(),
            buffer,
            _heap: heap
        })
    }

    pub(crate) fn resource_options(_info: &gpu::BufferInfo) -> objc2_metal::MTLResourceOptions {
        let options = objc2_metal::MTLResourceOptions::empty();
        options
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLBuffer> {
        &self.buffer
    }
}

impl gpu::Buffer for MTLBuffer {
    fn info(&self) -> &gpu::BufferInfo {
        &self.info
    }

    unsafe fn map(&self, offset: u64, _length: u64, _invalidate: bool) -> Option<*mut c_void> {
        if self.buffer.storageMode() == objc2_metal::MTLStorageMode::Private {
            return None;
        }
        let ptr = self.buffer.contents();
        /* objc2_metal marks the pointer incorrectly as
        if ptr == std::ptr::null_mut() {
            return None;
        }*/
        return Some(ptr.as_ptr().offset(offset as isize));
    }

    unsafe fn unmap(&self, _offset: u64, _length: u64, _flush: bool) {
    }
}

impl Hash for MTLBuffer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
    }
}

impl PartialEq<MTLBuffer> for MTLBuffer {
    fn eq(&self, other: &MTLBuffer) -> bool {
        self.buffer == other.buffer
    }
}

impl Eq for MTLBuffer {}
