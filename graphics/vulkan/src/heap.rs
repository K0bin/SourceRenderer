use std::{sync::Arc, ffi::c_void};

use sourcerenderer_core::gpu::*;

use super::*;

use ash::vk;

pub(crate) enum ResourceMemory<'a> {
    Dedicated {
        memory_type_index: u32
    },
    Suballocated {
        memory: &'a VkMemoryHeap,
        offset: u64
    }
}

pub struct VkMemoryHeap {
    device: Arc<RawVkDevice>,
    memory: vk::DeviceMemory,
    memory_type_index: u32,
    memory_properties: vk::MemoryPropertyFlags,
    map_ptr: Option<*mut c_void>
}

unsafe impl Send for VkMemoryHeap {}
unsafe impl Sync for VkMemoryHeap {}

impl Drop for VkMemoryHeap {
    fn drop(&mut self) {
        unsafe {
            if let Some(_map_ptr) = self.map_ptr {
                self.device.unmap_memory(self.memory);
            }

            self.device.free_memory(self.memory, None);
        }
    }
}

impl VkMemoryHeap {
    pub unsafe fn new(device: &Arc<RawVkDevice>, memory_type_index: u32, size: u64) -> Result<Self, OutOfMemoryError> {
        let mut flags_info = vk::MemoryAllocateFlagsInfo {
            flags: vk::MemoryAllocateFlags::DEVICE_ADDRESS,
            device_mask: 0u32,
            ..Default::default()
        };
        if !device.features.contains(VkFeatures::BDA) {
            flags_info.flags &= !vk::MemoryAllocateFlags::DEVICE_ADDRESS;
        }

        let memory_info = vk::MemoryAllocateInfo {
            allocation_size: size,
            memory_type_index,
            p_next: &flags_info as *const vk::MemoryAllocateFlagsInfo as *const c_void,
            ..Default::default()
        };
        let memory_result = device.allocate_memory(&memory_info, None);
        if let Err(e) = memory_result {
            if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                return Err(OutOfMemoryError {});
            }
        }
        let memory = memory_result.unwrap();

        let mut memory_info = vk::PhysicalDeviceMemoryProperties2::default();
        device.instance.get_physical_device_memory_properties2(device.physical_device, &mut memory_info);
        let memory_type_info = &memory_info.memory_properties.memory_types[memory_type_index as usize];

        let map_ptr: Option<*mut c_void> = if memory_type_info.property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
            Some(device.map_memory(memory, 0u64, size, vk::MemoryMapFlags::empty()).unwrap())
        } else {
            None
        };

        Ok(Self {
            device: device.clone(),
            memory,
            memory_type_index,
            memory_properties: memory_type_info.property_flags,
            map_ptr
        })
    }

    pub(crate) fn handle(&self) -> vk::DeviceMemory {
        self.memory
    }

    pub(crate) fn properties(&self) -> vk::MemoryPropertyFlags {
        self.memory_properties
    }

    pub(crate) unsafe fn map_ptr(&self, offset: u64) -> Option<*mut c_void> {
        self.map_ptr.map(|map_ptr| map_ptr.add(offset as usize))
    }
}

impl Heap<VkBackend> for VkMemoryHeap {
    fn memory_type_index(&self) -> u32 {
        self.memory_type_index
    }

    unsafe fn create_buffer(&self, info: &BufferInfo, offset: u64, name: Option<&str>) -> Result<VkBuffer, OutOfMemoryError> {
        VkBuffer::new(&self.device, ResourceMemory::Suballocated { memory: self, offset }, info, name)
    }

    unsafe fn create_texture(&self, info: &TextureInfo, offset: u64, name: Option<&str>) -> Result<VkTexture, OutOfMemoryError> {
        VkTexture::new(&self.device, info, ResourceMemory::Suballocated { memory: self, offset }, name)
    }
}
