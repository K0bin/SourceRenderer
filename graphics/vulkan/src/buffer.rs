use std::sync::Arc;

use ash::vk;

use sourcerenderer_core::graphics::{ Buffer, BufferUsage, MemoryUsage };

use crate::VkDevice;
use crate::raw::*;
use crate::VkBackend;
use crate::device::memory_usage_to_vma;

pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vk_mem::Allocation,
  allocation_info: vk_mem::AllocationInfo,
  device: Arc<RawVkDevice>,
  map_ptr: Option<*mut u8>,
  is_coherent: bool,
  memory_usage: MemoryUsage
}

impl VkBuffer {
  pub fn new(device: &Arc<RawVkDevice>, size: usize, memory_usage: MemoryUsage, allocator: &vk_mem::Allocator, usage: BufferUsage) -> Self {
    let buffer_info = vk::BufferCreateInfo {
      size: size as u64,
      usage: buffer_usage_to_vk(usage),
      ..Default::default()
    };
    let allocation_info = vk_mem::AllocationCreateInfo {
      usage: memory_usage_to_vma(memory_usage),
      ..Default::default()

    };
    let (buffer, allocation, allocation_info) = allocator.create_buffer(&buffer_info, &allocation_info).expect("Failed to create buffer.");

    let map_ptr: Option<*mut u8> = if memory_usage != MemoryUsage::GpuOnly {
      Some(allocator.map_memory(&allocation).unwrap())
    } else {
      None
    };

    let is_coherent = if memory_usage != MemoryUsage::GpuOnly {
      let memory_type = allocation_info.get_memory_type();
      let memory_properties = allocator.get_memory_type_properties(memory_type).unwrap();
      memory_properties.intersects(vk::MemoryPropertyFlags::HOST_COHERENT)
    } else {
      false
    };

    return VkBuffer {
      buffer,
      allocation,
      allocation_info,
      device: device.clone(),
      map_ptr,
      is_coherent,
      memory_usage
    };
  }

  pub fn get_handle(&self) -> &vk::Buffer {
    return &self.buffer;
  }
}

impl Drop for VkBuffer {
  fn drop(&mut self) {
    unsafe {
      self.device.allocator.destroy_buffer(self.buffer, &self.allocation).unwrap();
    }
  }
}

impl Buffer for VkBuffer {
  fn map(&self) -> Option<*mut u8> {
    if !self.is_coherent && (self.memory_usage == MemoryUsage::CpuToGpu || self.memory_usage == MemoryUsage::CpuOnly) {
      let mut allocator = &self.device.allocator;
      allocator.invalidate_allocation(&self.allocation, self.allocation_info.get_offset(), self.allocation_info.get_size()).unwrap();
    }
    return self.map_ptr;
  }

  fn unmap(&self) {
    if !self.is_coherent && (self.memory_usage == MemoryUsage::CpuToGpu || self.memory_usage == MemoryUsage::CpuOnly) {
      let mut allocator = &self.device.allocator;
      allocator.flush_allocation(&self.allocation, self.allocation_info.get_offset(), self.allocation_info.get_size()).unwrap();
    }
  }
}

pub fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
  use self::vk::BufferUsageFlags as VkUsage;
  let usage_bits = usage.bits();
  let mut flags = 0u32;
  flags |= usage_bits.rotate_left(VkUsage::VERTEX_BUFFER.as_raw().trailing_zeros() - BufferUsage::VERTEX.bits().trailing_zeros()) & VkUsage::VERTEX_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::INDEX_BUFFER.as_raw().trailing_zeros() - BufferUsage::INDEX.bits().trailing_zeros()) & VkUsage::INDEX_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::UNIFORM_BUFFER.as_raw().trailing_zeros() - BufferUsage::CONSTANT.bits().trailing_zeros()) & VkUsage::UNIFORM_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::INDIRECT_BUFFER.as_raw().trailing_zeros() - BufferUsage::INDIRECT.bits().trailing_zeros()) & VkUsage::INDIRECT_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros() - BufferUsage::STORAGE.bits().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::STORAGE_TEXEL.bits().trailing_zeros() - VkUsage::STORAGE_TEXEL_BUFFER.as_raw().trailing_zeros()) & VkUsage::STORAGE_TEXEL_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::UNIFORM_TEXEL.bits().trailing_zeros() - VkUsage::UNIFORM_TEXEL_BUFFER.as_raw().trailing_zeros()) & VkUsage::UNIFORM_TEXEL_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COPY_SRC.bits().trailing_zeros() - VkUsage::TRANSFER_SRC.as_raw().trailing_zeros()) & VkUsage::TRANSFER_SRC.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COPY_DST.bits().trailing_zeros() - VkUsage::TRANSFER_DST.as_raw().trailing_zeros()) & VkUsage::TRANSFER_DST.as_raw();
  return vk::BufferUsageFlags::from_raw(flags);
}
