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
use std::fmt::{Debug, Display};
use bitflags::_core::fmt::Formatter;
use ash::version::{InstanceV1_0, InstanceV1_1};
use std::cmp::max;
use bitflags::_core::mem::ManuallyDrop;
use std::hash::{Hash, Hasher};

pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vk_mem::Allocation,
  allocation_info: vk_mem::AllocationInfo,
  device: Arc<RawVkDevice>,
  map_ptr: Option<*mut u8>,
  is_coherent: bool,
  memory_usage: MemoryUsage
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
  pub fn new(device: &Arc<RawVkDevice>, size: usize, memory_usage: MemoryUsage, buffer_usage: BufferUsage, allocator: &vk_mem::Allocator) -> Self {
    let buffer_info = vk::BufferCreateInfo {
      size: size as u64,
      usage: buffer_usage_to_vk(buffer_usage),
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

fn align_up(value: usize, alignment: usize) -> usize {
  if alignment == 0 {
    return value
  }
  if value == 0 {
    return 0
  }
  (value + alignment - 1) & !(alignment - 1)
}

fn align_down(value: usize, alignment: usize) -> usize {
  if alignment == 0 {
    return value
  }
  (value / alignment) * alignment
}

fn align_up_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
    return value
  }
  if value == 0 {
    return 0
  }
  (value + alignment - 1) & (alignment - 1)
}

fn align_down_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
    return value
  }
  (value / alignment) * alignment
}

fn align_up_64(value: u64, alignment: u64) -> u64 {
  if alignment == 0 {
    return value
  }
  (value + alignment - 1) & (alignment - 1)
}

fn align_down_64(value: u64, alignment: u64) -> u64 {
  if alignment == 0 {
    return value
  }
  (value / alignment) * alignment
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
      existing_slice.length += self.length;
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

impl Debug for VkBufferSlice {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    write!(f, "(Buffer Slice: {}-{} (length: {}))", self.offset, self.offset + self.length, self.length)
  }
}

impl Hash for VkBufferSlice {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.buffer.hash(state);
    self.offset.hash(state);
    self.length.hash(state);
  }
}

impl PartialEq for VkBufferSlice {
  fn eq(&self, other: &Self) -> bool {
    self.buffer == other.buffer
      && self.length == other.length
      && self.offset == other.offset
  }
}

impl Eq for VkBufferSlice {}

impl VkBufferSlice {
  pub fn get_buffer(&self) -> &Arc<VkBuffer> {
    &self.buffer
  }

  pub fn get_offset_and_length(&self) -> (usize, usize) {
    (self.offset, self.length)
  }

  pub fn get_offset(&self) -> usize {
    self.offset
  }

  pub fn get_length(&self) -> usize {
    self.length
  }
}

fn get_slice(buffer: &VkSlicedBuffer, length: usize, offset_alignment: usize) -> Option<VkBufferSlice> {
  let mut guard = buffer.lock().unwrap();

  let mut slice_option = guard
      .iter_mut()
      .enumerate()
      .find(|(index, s)|
          s.length > length &&
          s.length - ((s.offset + s.length - length) - align_down(s.offset + s.length - length, offset_alignment)) >= length);
  if let Some((index, existing_slice)) = slice_option {
    return if existing_slice.length > length {
      let offset = existing_slice.length - length;
      let aligned_offset = align_down(offset, offset_alignment);
      debug_assert!(aligned_offset <= offset);
      let alignment_delta = offset - aligned_offset;

      let new_slice = VkBufferSlice {
        buffer: existing_slice.buffer.clone(),
        slices: buffer.clone(),
        offset: aligned_offset,
        length: length + alignment_delta
      };
      existing_slice.length -= length + alignment_delta;
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
  buffers: Mutex<HashMap<(MemoryUsage, BufferUsage), Vec<VkSlicedBuffer>>>,
  device_limits: vk::PhysicalDeviceLimits
}

impl BufferAllocator {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let buffers = HashMap::<(MemoryUsage, BufferUsage), Vec<VkSlicedBuffer>>::new();
    let mut limits2 = vk::PhysicalDeviceProperties2 {
      ..Default::default()
    };

    unsafe {
      device.instance.get_physical_device_properties2(device.physical_device, &mut limits2)
    }

    BufferAllocator {
      device: device.clone(),
      buffers: Mutex::new(buffers),
      device_limits: limits2.properties.limits.clone()
    }
  }

  pub fn get_slice(&self, usage: MemoryUsage, buffer_usage: BufferUsage, length: usize) -> VkBufferSlice {
    if length > UNIQUE_BUFFER_THRESHOLD {
      let buffer = Arc::new(VkBuffer::new(&self.device, length, usage, buffer_usage, &self.device.allocator));
      return VkBufferSlice {
        buffer,
        slices: Arc::new(Mutex::new(Vec::new())),
        offset: 0,
        length
      };
    }

    let mut alignment: usize = 0;
    if (buffer_usage & BufferUsage::CONSTANT) == BufferUsage::CONSTANT {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_uniform_buffer_offset_alignment as usize);
    }

    {
      let mut guard = self.buffers.lock().unwrap();
      let mut matching_buffers = guard.entry((usage, buffer_usage)).or_default();
      for buffer in matching_buffers.iter() {
        if let Some(slice) = get_slice(buffer, length, alignment) {
          return slice;
        }
      }

      let buffer = Arc::new(VkBuffer::new(&self.device, UNIQUE_BUFFER_THRESHOLD, usage, buffer_usage, &self.device.allocator));
      let mut slices = Arc::new(Mutex::new(Vec::new()));

      if length != UNIQUE_BUFFER_THRESHOLD {
        let mut slices_guard = slices.lock().unwrap();
        slices_guard.push(VkBufferSlice {
          buffer: buffer.clone(),
          slices: slices.clone(),
          offset: length,
          length: UNIQUE_BUFFER_THRESHOLD - length
        });
      }

      matching_buffers.push(slices.clone());
      return VkBufferSlice {
        buffer,
        slices,
        offset: 0,
        length
      };
    }
  }
}
