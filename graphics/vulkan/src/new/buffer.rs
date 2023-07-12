use std::{
    ffi::{CString, c_void},
    hash::{
        Hash,
        Hasher,
    },
    mem::MaybeUninit,
    sync::Arc,
};

use ash::vk::{
    self,
    BufferDeviceAddressInfo,
    Handle,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::*;

use super::*;

pub struct VkBuffer {
    buffer: vk::Buffer,
    device: Arc<RawVkDevice>,
    map_ptr: Option<*mut c_void>,
    info: BufferInfo,
    va: Option<vk::DeviceSize>,
    memory: vk::DeviceMemory,
    memory_offset: u64,
    is_memory_owned: bool,
    is_coherent: bool,
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
    pub(crate) unsafe fn new(
        device: &Arc<RawVkDevice>,
        memory: ResourceMemory,
        info: &BufferInfo,
        name: Option<&str>,
    ) -> Result<Self, OutOfMemoryError> {
        let mut queue_families = SmallVec::<[u32; 3]>::new();
        let mut sharing_mode = vk::SharingMode::EXCLUSIVE;
        if info.sharing_mode == QueueSharingMode::Concurrent && (device.transfer_queue_info.is_some() || device.compute_queue_info.is_some()) {
            sharing_mode = vk::SharingMode::CONCURRENT;
            queue_families.push(device.graphics_queue_info.queue_family_index as u32);
            if let Some(info) = device.transfer_queue_info.as_ref() {
                queue_families.push(info.queue_family_index as u32);
            }
            if let Some(info) = device.compute_queue_info.as_ref() {
                queue_families.push(info.queue_family_index as u32);
            }
        }

        let buffer_info = vk::BufferCreateInfo {
            size: info.size as u64,
            usage: buffer_usage_to_vk(
                info.usage,
                device.features.contains(VkFeatures::RAY_TRACING),
            ),
            sharing_mode,
            p_queue_family_indices: queue_families.as_ptr(),
            queue_family_index_count: queue_families.len() as u32,
            ..Default::default()
        };
        let buffer_res = device.create_buffer(&buffer_info, None);
        if let Err(e) = buffer_res {
            if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                return Err(OutOfMemoryError {});
            }
        }
        let buffer = buffer_res.unwrap();

        let is_host_coherent;
        let mut map_ptr: Option<*mut c_void> = None;
        let vk_memory: vk::DeviceMemory;
        let mut suballocation_offset = 0u64;
        let mut is_memory_owned = false;

        match memory {
            ResourceMemory::Dedicated {
                memory_type_index
            } => {
                let requirements_info = vk::BufferMemoryRequirementsInfo2 {
                    buffer: buffer,
                    ..Default::default()
                };
                let mut requirements = vk::MemoryRequirements2::default();
                device.get_buffer_memory_requirements2(&requirements_info, &mut requirements);
                assert!((requirements.memory_requirements.memory_type_bits & (1 << memory_type_index)) != 0);

                let dedicated_alloc = vk::MemoryDedicatedAllocateInfo {
                    buffer: buffer,
                    ..Default::default()
                };
                let memory_info = vk::MemoryAllocateInfo {
                    allocation_size: requirements.memory_requirements.size,
                    memory_type_index,
                    p_next: &dedicated_alloc as *const vk::MemoryDedicatedAllocateInfo as *const c_void,
                    ..Default::default()
                };
                let memory_result: Result<vk::DeviceMemory, vk::Result> = device.allocate_memory(&memory_info, None);
                if let Err(e) = memory_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(OutOfMemoryError {});
                    }
                }
                vk_memory = memory_result.unwrap();

                let bind_result = device.bind_buffer_memory2(&[
                    vk::BindBufferMemoryInfo {
                        buffer,
                        memory: vk_memory,
                        memory_offset: 0u64,
                        ..Default::default()
                    }
                ]);
                if let Err(e) = bind_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(OutOfMemoryError {});
                    }
                }

                let mut memory_info = vk::PhysicalDeviceMemoryProperties2::default();
                device.instance.get_physical_device_memory_properties2(device.physical_device, &mut memory_info);
                let memory_type_info = &memory_info.memory_properties.memory_types[memory_type_index as usize];
                let is_host_visible = memory_type_info.property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
                is_host_coherent = memory_type_info.property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);

                if is_host_visible {
                    map_ptr = Some(device.map_memory(vk_memory, 0, info.size, vk::MemoryMapFlags::empty()).unwrap());
                }
                is_memory_owned = true;
            }

            ResourceMemory::Suballocated {
                memory,
                offset
            } => {
                let bind_result = device.bind_buffer_memory2(&[
                    vk::BindBufferMemoryInfo {
                        buffer,
                        memory: memory.handle(),
                        memory_offset: offset,
                        ..Default::default()
                    }
                ]);
                if let Err(e) = bind_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(OutOfMemoryError {});
                    }
                }

                is_host_coherent = memory.properties().contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);

                map_ptr = memory.map_ptr(offset);
                suballocation_offset = offset;
                vk_memory = memory.handle();
            }
        }

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                debug_utils
                    .debug_utils_loader
                    .set_debug_utils_object_name(
                        device.handle(),
                        &vk::DebugUtilsObjectNameInfoEXT {
                            object_type: vk::ObjectType::BUFFER,
                            object_handle: buffer.as_raw(),
                            p_object_name: name_cstring.as_ptr(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }

        let va = if buffer_info
            .usage
            .contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
        {
            device.rt.as_ref().map(|rt| unsafe {
                rt.bda.get_buffer_device_address(&BufferDeviceAddressInfo {
                    buffer,
                    ..Default::default()
                })
            })
        } else {
            None
        };

        Ok(VkBuffer {
            buffer,
            device: device.clone(),
            map_ptr,
            info: info.clone(),
            va,
            memory_offset: suballocation_offset,
            memory: vk_memory,
            is_coherent: is_host_coherent,
            is_memory_owned
        })
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer
    }

    pub fn va(&self) -> Option<vk::DeviceAddress> {
        self.va
    }

    pub(crate) fn info(&self) -> &BufferInfo {
        &self.info
    }
}

impl Drop for VkBuffer {
    fn drop(&mut self) {
        unsafe {
            if self.is_memory_owned {
                if self.map_ptr.is_some() {
                    self.device.unmap_memory(self.memory);
                }
                self.device.free_memory(self.memory, None);
            }

            self.device.destroy_buffer(self.buffer, None);
        }
    }
}

impl Hash for VkBuffer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
    }
}

impl PartialEq for VkBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
    }
}

impl Eq for VkBuffer {}

impl Buffer for VkBuffer {
    fn info(&self) -> &BufferInfo {
        &self.info
    }

    unsafe fn map_unsafe(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void> {
        assert!(self.map_ptr.is_some());
        if invalidate && !self.is_coherent {
            self.device.invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange {
                memory: self.memory,
                offset: offset + self.memory_offset,
                size: length.min(self.info.size),
                ..Default::default()
            }]).unwrap();
        }
        self.map_ptr.map(|ptr| ptr.add(offset as usize))
    }

    unsafe fn unmap_unsafe(&self, offset: u64, length: u64, flush: bool) {
        assert!(self.map_ptr.is_some());
        if !flush || self.is_coherent {
            return;
        }
        self.device.flush_mapped_memory_ranges(&[vk::MappedMemoryRange {
            memory: self.memory,
            offset: offset + self.memory_offset,
            size: length.min(self.info.size),
            ..Default::default()
        }]).unwrap();
    }
}

pub fn buffer_usage_to_vk(usage: BufferUsage, rt_supported: bool) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();

    if usage.contains(BufferUsage::STORAGE) {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }

    if usage.contains(BufferUsage::CONSTANT) {
        flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
    }

    if usage.contains(BufferUsage::VERTEX) {
        flags |= vk::BufferUsageFlags::VERTEX_BUFFER;

        if rt_supported {
            flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS_EXT;
        }
    }

    if usage.contains(BufferUsage::INDEX) {
        flags |= vk::BufferUsageFlags::INDEX_BUFFER;

        if rt_supported {
            flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS_EXT;
        }
    }

    if usage.contains(BufferUsage::INDIRECT) {
        flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
    }

    if usage.contains(BufferUsage::COPY_SRC) {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }

    if usage.contains(BufferUsage::COPY_DST) {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }

    if usage.contains(BufferUsage::ACCELERATION_STRUCTURE) {
        flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS_EXT;
    }

    if usage.contains(BufferUsage::ACCELERATION_STRUCTURE_BUILD) {
        flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS_EXT;
    }

    if usage.contains(BufferUsage::SHADER_BINDING_TABLE) {
        flags |= vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS_EXT;
    }

    flags
}

pub(crate) fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    if value == 0 {
        return 0;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}

pub(crate) fn align_up_32(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    if value == 0 {
        return 0;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_32(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}

pub(crate) fn align_up_64(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_64(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}
