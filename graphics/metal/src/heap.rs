use std::sync::Arc;

use metal;
use metal::foreign_types::ForeignType;
use metal::objc::{msg_send, sel, sel_impl};

use sourcerenderer_core::gpu::{self, OutOfMemoryError};

use super::*;

pub(crate) enum ResourceMemory<'a> {
    Dedicated {
        device: &'a metal::DeviceRef,
        options: metal::MTLResourceOptions
    },
    Suballocated {
        memory: &'a MTLHeap,
        offset: u64
    }
}

pub struct MTLHeap {
    heap: metal::Heap,
    memory_type_index: u32,
    options: metal::MTLResourceOptions,
    shared: Arc<MTLShared>
}

impl MTLHeap {
    pub(crate) fn new(device: &metal::DeviceRef, shared: &Arc<MTLShared>, size: u64, memory_type_index: u32, cached: bool, memory_kind: gpu::MemoryKind, options: metal::MTLResourceOptions) -> Result<Self, gpu::OutOfMemoryError> {
        let mut descriptor = metal::HeapDescriptor::new();
        descriptor.set_size(size);
        unsafe {
            let _: () = msg_send![&descriptor as &metal::HeapDescriptorRef, setType: metal::MTLHeapType::Placement];
        }

        if device.has_unified_memory() {
            if memory_kind == gpu::MemoryKind::VRAM {
                descriptor.set_storage_mode(metal::MTLStorageMode::Private);
            } else {
                descriptor.set_storage_mode(metal::MTLStorageMode::Shared);
            }
        } else {
            descriptor.set_storage_mode(metal::MTLStorageMode::Shared);
        }
        descriptor.set_cpu_cache_mode(if cached { metal::MTLCPUCacheMode::DefaultCache } else { metal::MTLCPUCacheMode::WriteCombined });
        let heap = device.new_heap(&descriptor);
        if heap.as_ptr() == std::ptr::null_mut() {
            return Err(OutOfMemoryError {});
        }
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

    pub(crate) fn handle(&self) -> &metal::HeapRef {
        &self.heap
    }

    pub(crate) fn resource_options(&self) -> metal::MTLResourceOptions {
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
        let index = list.iter().enumerate().find_map(|(index, heap)| if heap.as_ptr() == self.heap.as_ptr() {
            Some(index)
        } else {
            None
        });
        list.remove(index.unwrap());
    }
}
