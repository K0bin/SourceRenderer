use std::{
    ffi::CString,
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
    allocation: vma_sys::VmaAllocation,
    device: Arc<RawVkDevice>,
    map_ptr: Option<*mut u8>,
    memory_usage: MemoryUsage,
    info: BufferInfo,
    va: Option<vk::DeviceSize>,
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
    pub fn new(
        device: &Arc<RawVkDevice>,
        memory_usage: MemoryUsage,
        info: &BufferInfo,
        pool: Option<vma_sys::VmaPool>,
        name: Option<&str>,
    ) -> Self {
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
        let vk_mem_flags = memory_usage_to_vma(memory_usage);
        let allocation_create_info = vma_sys::VmaAllocationCreateInfo {
            flags: if memory_usage != MemoryUsage::VRAM {
                vma_sys::VmaAllocationCreateFlagBits_VMA_ALLOCATION_CREATE_MAPPED_BIT as u32
            } else {
                0
            },
            usage: vma_sys::VmaMemoryUsage_VMA_MEMORY_USAGE_UNKNOWN,
            preferredFlags: vk_mem_flags.preferred,
            requiredFlags: vk_mem_flags.required,
            memoryTypeBits: 0,
            pool: pool.unwrap_or(std::ptr::null_mut()),
            pUserData: std::ptr::null_mut(),
            priority: 0f32,
        };
        let mut buffer: vk::Buffer = vk::Buffer::null();
        let mut allocation: vma_sys::VmaAllocation = std::ptr::null_mut();
        let mut allocation_info_uninit: MaybeUninit<vma_sys::VmaAllocationInfo> =
            MaybeUninit::uninit();
        let allocation_info: vma_sys::VmaAllocationInfo;
        unsafe {
            assert_eq!(
                vma_sys::vmaCreateBuffer(
                    device.allocator,
                    &buffer_info,
                    &allocation_create_info,
                    &mut buffer,
                    &mut allocation,
                    allocation_info_uninit.as_mut_ptr()
                ),
                vk::Result::SUCCESS
            );
            allocation_info = allocation_info_uninit.assume_init();
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
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
        }

        let map_ptr: Option<*mut u8> = unsafe {
            if memory_usage != MemoryUsage::VRAM
                && allocation_info.pMappedData != std::ptr::null_mut()
            {
                Some(std::mem::transmute(allocation_info.pMappedData))
            } else {
                None
            }
        };

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

        VkBuffer {
            buffer,
            allocation,
            device: device.clone(),
            map_ptr,
            memory_usage,
            info: info.clone(),
            va,
        }
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
            // VMA_ALLOCATION_CREATE_MAPPED_BIT will get automatically unmapped
            vma_sys::vmaDestroyBuffer(self.device.allocator, self.buffer, self.allocation);
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

    unsafe fn map_unsafe(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut u8> {
        if !invalidate {
            let allocator = self.device.allocator;
            assert_eq!(
                vma_sys::vmaInvalidateAllocation(
                    allocator,
                    self.allocation,
                    offset as u64,
                    (self.info.size as u64 - offset).min(length) as u64
                ),
                vk::Result::SUCCESS
            );
        }
        self.map_ptr.map(|ptr| ptr.add(offset as usize))
    }

    unsafe fn unmap_unsafe(&self, offset: u64, length: u64, flush: bool) {
        if !flush {
            return;
        }
        let allocator = self.device.allocator;
        assert_eq!(
            vma_sys::vmaFlushAllocation(
                allocator,
                self.allocation,
                offset as u64,
                (self.info.size as u64 - offset).min(length) as u64
            ),
            vk::Result::SUCCESS
        );
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
