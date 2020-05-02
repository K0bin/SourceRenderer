use std::sync::{Arc, Mutex};

use ash::vk;

use sourcerenderer_core::graphics::{Buffer, BufferUsage, MemoryUsage, MappedBuffer};

use crate::VkDevice;
use crate::raw::*;
use crate::VkBackend;
use crate::device::memory_usage_to_vma;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use sourcerenderer_core::pool::Recyclable;
use std::process::exit;
use std::collections::HashMap;
use ash::vk::BufferUsageFlags;

pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vk_mem::Allocation,
  allocation_info: vk_mem::AllocationInfo,
  device: Arc<RawVkDevice>,
  map_ptr: Option<*mut u8>,
  is_coherent: bool,
  memory_usage: MemoryUsage,
  slices: Mutex<Vec<VkBufferSlice>>
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
  pub fn new(device: &Arc<RawVkDevice>, size: usize, memory_usage: MemoryUsage, allocator: &vk_mem::Allocator) -> Self {
    let buffer_info = vk::BufferCreateInfo {
      size: size as u64,
      usage: BufferUsageFlags::INDEX_BUFFER
        | BufferUsageFlags::VERTEX_BUFFER
        | BufferUsageFlags::UNIFORM_BUFFER
        | BufferUsageFlags::TRANSFER_SRC
        | BufferUsageFlags::TRANSFER_DST,
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
      memory_usage,
      slices: Mutex::new(Vec::new())
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

impl Buffer for VkBufferSlice {
  fn map<T>(&self) -> Option<MappedBuffer<Self, T>>
    where T: Sized {
    MappedBuffer::new(self)
  }

  unsafe fn map_unsafe(&self) -> Option<*mut u8> {
    if !self.buffer.is_coherent && (self.buffer.memory_usage == MemoryUsage::CpuToGpu || self.buffer.memory_usage == MemoryUsage::CpuOnly) {
      let mut allocator = &self.buffer.device.allocator;
      allocator.invalidate_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length).unwrap();
    }
    return self.buffer.map_ptr.map(|ptr| ptr.add(self.offset));
  }

  unsafe fn unmap_unsafe(&self) {
    if !self.buffer.is_coherent && (self.buffer.memory_usage == MemoryUsage::CpuToGpu || self.buffer.memory_usage == MemoryUsage::CpuOnly) {
      let mut allocator = &self.buffer.device.allocator;
      allocator.flush_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length).unwrap();
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

type VkSlicedBuffer = Arc<Mutex<Vec<VkBufferSlice>>>;
pub struct VkBufferSlice {
  buffer: Arc<VkBuffer>,
  slices: VkSlicedBuffer,
  offset: usize,
  length: usize
}

impl Drop for VkBufferSlice {
  fn drop(&mut self) {
    let mut guard = self.slices.lock().unwrap();
    let end = guard.iter_mut().find(|s| s.offset + s.length == self.offset);
    if let Some(mut existing_slice) = end {
      existing_slice.length += self.length;
      return;
    }
    let start = guard.iter_mut().find(|s| s.offset == self.offset + self.length);
    if let Some(mut existing_slice) = start {
      existing_slice.offset -= self.length;
      return;
    }
    guard.push(VkBufferSlice {
      buffer: self.buffer.clone(),
      slices: self.slices.clone(),
      offset: self.offset,
      length: self.length
    });
  }
}

impl VkBufferSlice {
  pub fn get_buffer(&self) -> &Arc<VkBuffer> {
    &self.buffer
  }

  pub fn get_offset_and_length(&self) -> (usize, usize) {
    (self.offset, self.length)
  }
}

fn get_slice(buffer: &VkSlicedBuffer, length: usize) -> Option<VkBufferSlice> {
  let mut guard = buffer.lock().unwrap();
  let mut slice_option = guard.iter_mut().enumerate().find(|(index, s)| s.length >= length);
  if let Some((index, existing_slice)) = slice_option {
    return if existing_slice.length > length {
      let new_slice = VkBufferSlice {
        buffer: existing_slice.buffer.clone(),
        slices: buffer.clone(),
        offset: existing_slice.offset,
        length
      };
      existing_slice.length -= length;
      existing_slice.offset += length;
      Some(new_slice)
    } else {
      Some(guard.remove(index))
    }
  }
  return None;
}

const UNIQUE_BUFFER_THRESHOLD: usize = 16384;
pub struct BufferAllocator {
  device: Arc<RawVkDevice>,
  buffers: Mutex<HashMap<MemoryUsage, Vec<VkSlicedBuffer>>>
}

impl BufferAllocator {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let mut buffers = HashMap::<MemoryUsage, Vec<VkSlicedBuffer>>::new();
    buffers.insert(MemoryUsage::CpuToGpu, Vec::new());
    buffers.insert(MemoryUsage::CpuOnly, Vec::new());
    BufferAllocator {
      device: device.clone(),
      buffers: Mutex::new(buffers)
    }
  }

  pub fn get_slice(&self, usage: MemoryUsage, length: usize) -> VkBufferSlice {
    if length > UNIQUE_BUFFER_THRESHOLD {
      let buffer = Arc::new(VkBuffer::new(&self.device, length, usage, &self.device.allocator));
      return VkBufferSlice {
        buffer,
        slices: Arc::new(Mutex::new(Vec::new())),
        offset: 0,
        length
      };
    }

    {
      let mut guard = self.buffers.lock().unwrap();
      let matching_buffers = guard.get(&usage).expect("unsupported memory usage");
      for buffer in matching_buffers {
        if let Some(slice) = get_slice(buffer, length) {
          return slice;
        }
      }
    }
    let buffer = Arc::new(VkBuffer::new(&self.device, UNIQUE_BUFFER_THRESHOLD, usage, &self.device.allocator));
    return VkBufferSlice {
      buffer,
      slices: Arc::new(Mutex::new(Vec::new())),
      offset: 0,
      length: UNIQUE_BUFFER_THRESHOLD
    };
  }
}
