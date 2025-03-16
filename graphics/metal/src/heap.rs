use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSUInteger;
use objc2_metal::{self, MTLDevice as _};

use sourcerenderer_core::gpu;

use super::*;

pub(crate) enum ResourceMemory<'a> {
    Dedicated {
        device: &'a ProtocolObject<dyn objc2_metal::MTLDevice>,
        options: objc2_metal::MTLResourceOptions
    },
    Suballocated {
        memory: &'a MTLHeap,
        offset: u64
    }
}

pub struct MTLHeap {
    heap: Retained<ProtocolObject<dyn objc2_metal::MTLHeap>>,
    memory_type_index: u32,
    options: objc2_metal::MTLResourceOptions,
    shared: Arc<MTLShared>
}

unsafe impl Send for MTLHeap {}
unsafe impl Sync for MTLHeap {}

impl MTLHeap {
    pub(crate) unsafe fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, shared: &Arc<MTLShared>, size: u64, memory_type_index: u32, cached: bool, memory_kind: gpu::MemoryKind, mut options: objc2_metal::MTLResourceOptions) -> Result<Self, gpu::OutOfMemoryError> {
        let descriptor = objc2_metal::MTLHeapDescriptor::new();
        descriptor.setSize(size as NSUInteger);
        descriptor.setType(objc2_metal::MTLHeapType::Placement);

        options |= objc2_metal::MTLResourceOptions::HazardTrackingModeUntracked;

        if !device.hasUnifiedMemory() {
            if memory_kind == gpu::MemoryKind::VRAM {
                descriptor.setStorageMode(objc2_metal::MTLStorageMode::Private);
            } else {
                descriptor.setStorageMode(objc2_metal::MTLStorageMode::Shared);
            }
        } else {
            descriptor.setStorageMode(objc2_metal::MTLStorageMode::Shared);
        }
        descriptor.setCpuCacheMode(if cached { objc2_metal::MTLCPUCacheMode::DefaultCache } else { objc2_metal::MTLCPUCacheMode::WriteCombined });
        let heap_opt = device.newHeapWithDescriptor(&descriptor);
        if heap_opt.is_none() {
            return Err(gpu::OutOfMemoryError {});
        }
        let heap = heap_opt.unwrap();
        {
            let mut list = shared.heap_list.write().unwrap();
            list.push(heap.clone());
        }
        Ok(Self {
            heap,
            memory_type_index,
            options,
            shared: shared.clone()
        })
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLHeap> {
        &self.heap
    }

    pub(crate) fn resource_options(&self) -> objc2_metal::MTLResourceOptions {
        self.options
    }
}

impl gpu::Heap<MTLBackend> for MTLHeap {
    fn memory_type_index(&self) -> u32 {
        self.memory_type_index
    }

    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, offset: u64, name: Option<&str>) -> Result<MTLBuffer, gpu::OutOfMemoryError> {
        MTLBuffer::new(
            ResourceMemory::Suballocated {
                memory: self,
                offset: offset
            },
            info,
            name
        )
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, offset: u64, name: Option<&str>) -> Result<MTLTexture, gpu::OutOfMemoryError> {
        MTLTexture::new(
            ResourceMemory::Suballocated {
                memory: self,
                offset: offset
            },
            info,
            name
        )
    }
}

impl Drop for MTLHeap {
    fn drop(&mut self) {
        let mut list = self.shared.heap_list.write().unwrap();
        let index = list.iter().enumerate().find_map(|(index, heap)| if heap == &self.heap {
            Some(index)
        } else {
            None
        });
        list.remove(index.unwrap());
    }
}
