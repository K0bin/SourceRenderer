use std::sync::Arc;

use ash::vk;

use sourcerenderer_core::graphics::{ Buffer, BufferUsage, MemoryUsage };

use crate::VkDevice;
use crate::device::memory_usage_to_vma;

pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vk_mem::Allocation,
  allocation_info: vk_mem::AllocationInfo,
  device: Arc<VkDevice>
}

impl VkBuffer {
  pub fn new(device: Arc<VkDevice>, size: usize, memory_usage: MemoryUsage, allocator: &mut vk_mem::Allocator, usage: BufferUsage) -> Self {
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
    return VkBuffer {
      buffer: buffer,
      allocation: allocation,
      allocation_info: allocation_info,
      device: device
    };
  }
}

impl Drop for VkBuffer {
  fn drop(&mut self) {
    let mut allocator = self.device.get_allocator().lock().unwrap();
    allocator.destroy_buffer(self.buffer, &self.allocation).unwrap();
  }
}

impl Buffer for VkBuffer {

}

pub fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
  use vk::BufferUsageFlags as VkUsage;
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
