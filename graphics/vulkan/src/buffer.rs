use std::{collections::HashMap, fmt::Formatter, sync::{Arc, Mutex}};
use std::cmp::max;
use std::hash::{Hash, Hasher};
use std::fmt::Debug;
use std::ffi::CString;
use ash::vk::Handle;

use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use ash::{version::InstanceV1_1, vk};

use crate::raw::*;
use crate::device::memory_usage_to_vma;
use smallvec::SmallVec;
pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vk_mem::Allocation,
  allocation_info: vk_mem::AllocationInfo,
  device: Arc<RawVkDevice>,
  map_ptr: Option<*mut u8>,
  is_coherent: bool,
  memory_usage: MemoryUsage,
  info: BufferInfo,
  free_slices: Mutex<Vec<Arc<VkBufferSlice>>>,
  used_slices: Mutex<Vec<Arc<VkBufferSlice>>>
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
  pub fn new(device: &Arc<RawVkDevice>, slices: usize, memory_usage: MemoryUsage, info: &BufferInfo, allocator: &vk_mem::Allocator, name: Option<&str>) -> Arc<Self> {
    let mut queue_families = SmallVec::<[u32; 2]>::new();
    let mut sharing_mode = vk::SharingMode::EXCLUSIVE;
    if info.usage.contains(BufferUsage::COPY_SRC) {
      queue_families.push(device.graphics_queue_info.queue_family_index as u32);
      if let Some(info) = device.transfer_queue_info {
        sharing_mode = vk::SharingMode::CONCURRENT;
        queue_families.push(info.queue_family_index as u32);
      }
    }

    let buffer_info = vk::BufferCreateInfo {
      size: (info.size * slices) as u64,
      usage: buffer_usage_to_vk(info.usage),
      sharing_mode,
      p_queue_family_indices: queue_families.as_ptr(),
      queue_family_index_count: queue_families.len() as u32,
      ..Default::default()
    };
    let allocation_info = vk_mem::AllocationCreateInfo {
      usage: memory_usage_to_vma(memory_usage),
      ..Default::default()
    };
    let (buffer, allocation, allocation_info) = allocator.create_buffer(&buffer_info, &allocation_info).expect("Failed to create buffer.");
    if let Some(name) = name {
      if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
        let name_cstring = CString::new(name).unwrap();
        unsafe {
          debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
            object_type: vk::ObjectType::BUFFER,
            object_handle: buffer.as_raw(),
            p_object_name: name_cstring.as_ptr(),
            ..Default::default()
          }).unwrap();
        }
      }
    }

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

    let buffer = Arc::new(VkBuffer {
      buffer,
      allocation,
      allocation_info,
      device: device.clone(),
      map_ptr,
      is_coherent,
      memory_usage,
      info: info.clone(),
      free_slices: Mutex::new(Vec::with_capacity(slices)),
      used_slices: Mutex::new(Vec::with_capacity(slices))
    });

    {
      let mut slices_guard = buffer.free_slices.lock().unwrap();
      for i in 0..slices {
        slices_guard.push(Arc::new(VkBufferSlice {
          buffer: buffer.clone(),
          offset: i * info.size,
          length: info.size
        }));
      }
    }

    buffer
  }

  pub fn get_handle(&self) -> &vk::Buffer {
    &self.buffer
  }
}

impl Drop for VkBuffer {
  fn drop(&mut self) {
    self.device.allocator.destroy_buffer(self.buffer, &self.allocation).unwrap();
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
  fn map_mut<T>(&self) -> Option<MutMappedBuffer<Self, T>>
    where T: 'static + Send + Sync + Sized + Clone {
    MutMappedBuffer::new(self, true)
  }

  fn map<T>(&self) -> Option<MappedBuffer<Self, T>>
    where T: 'static + Send + Sync + Sized + Clone {
    MappedBuffer::new(self, true)
  }

  unsafe fn map_unsafe(&self, invalidate: bool) -> Option<*mut u8> {
    if !self.buffer.is_coherent &&
      (self.buffer.memory_usage == MemoryUsage::CpuToGpu
        || self.buffer.memory_usage == MemoryUsage::CpuOnly
        || self.buffer.memory_usage == MemoryUsage::GpuToCpu) {
      let allocator = &self.buffer.device.allocator;
      if invalidate {
        allocator.invalidate_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length).unwrap();
      }
    }
    self.buffer.map_ptr.map(|ptr| ptr.add(self.offset))
  }

  unsafe fn unmap_unsafe(&self, flush: bool) {
    if !self.buffer.is_coherent &&
      (self.buffer.memory_usage == MemoryUsage::CpuToGpu
        || self.buffer.memory_usage == MemoryUsage::CpuOnly
        || self.buffer.memory_usage == MemoryUsage::GpuToCpu) {
      let allocator = &self.buffer.device.allocator;
      if flush {
        allocator.flush_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length).unwrap();
      }
    }
  }

  fn get_length(&self) -> usize {
    self.length
  }
}

pub fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
  use self::vk::BufferUsageFlags as VkUsage;
  let usage_bits = usage.bits();
  let mut flags = 0u32;
  flags |= usage_bits.rotate_left(VkUsage::VERTEX_BUFFER.as_raw().trailing_zeros()   - BufferUsage::VERTEX.bits().trailing_zeros()) & VkUsage::VERTEX_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::INDEX_BUFFER.as_raw().trailing_zeros()    - BufferUsage::INDEX.bits().trailing_zeros()) & VkUsage::INDEX_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::VERTEX_SHADER_CONSTANT.bits().trailing_zeros() - VkUsage::UNIFORM_BUFFER.as_raw().trailing_zeros()) & VkUsage::UNIFORM_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::FRAGMENT_SHADER_CONSTANT.bits().trailing_zeros() - VkUsage::UNIFORM_BUFFER.as_raw().trailing_zeros()) & VkUsage::UNIFORM_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COMPUTE_SHADER_CONSTANT.bits().trailing_zeros() - VkUsage::UNIFORM_BUFFER.as_raw().trailing_zeros()) & VkUsage::UNIFORM_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::INDIRECT.bits().trailing_zeros()     - VkUsage::INDIRECT_BUFFER.as_raw().trailing_zeros()) & VkUsage::INDIRECT_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()  - BufferUsage::VERTEX_SHADER_STORAGE_READ.bits().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::VERTEX_SHADER_STORAGE_WRITE.bits().trailing_zeros() - VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()  - BufferUsage::FRAGMENT_SHADER_STORAGE_READ.bits().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()  - BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE.bits().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_left(VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()  - BufferUsage::COMPUTE_SHADER_STORAGE_READ.bits().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COMPUTE_SHADER_STORAGE_WRITE.bits().trailing_zeros() - VkUsage::STORAGE_BUFFER.as_raw().trailing_zeros()) & VkUsage::STORAGE_BUFFER.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COPY_SRC.bits().trailing_zeros() - VkUsage::TRANSFER_SRC.as_raw().trailing_zeros()) & VkUsage::TRANSFER_SRC.as_raw();
  flags |= usage_bits.rotate_right(BufferUsage::COPY_DST.bits().trailing_zeros() - VkUsage::TRANSFER_DST.as_raw().trailing_zeros()) & VkUsage::TRANSFER_DST.as_raw();
  vk::BufferUsageFlags::from_raw(flags)
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

pub struct VkBufferSlice {
  buffer: Arc<VkBuffer>,
  offset: usize,
  length: usize
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

const SLICED_BUFFER_SIZE: usize = 16384;
const BIG_BUFFER_SLAB_SIZE: usize = 4096;
const BUFFER_SLAB_SIZE: usize = 1024;
const SMALL_BUFFER_SLAB_SIZE: usize = 512;
const TINY_BUFFER_SLAB_SIZE: usize = 256;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
  memory_usage: MemoryUsage,
  buffer_usage: BufferUsage
}

pub struct BufferAllocator {
  device: Arc<RawVkDevice>,
  buffers: Mutex<HashMap<BufferKey, Vec<Arc<VkBuffer>>>>,
  device_limits: vk::PhysicalDeviceLimits,
  reuse_automatically: bool
}

impl BufferAllocator {
  pub fn new(device: &Arc<RawVkDevice>, reuse_automatically: bool) -> Self {
    let buffers: HashMap<BufferKey, Vec<Arc<VkBuffer>>> = HashMap::new();
    let mut limits2 = vk::PhysicalDeviceProperties2 {
      ..Default::default()
    };

    unsafe {
      device.instance.get_physical_device_properties2(device.physical_device, &mut limits2)
    }

    BufferAllocator {
      device: device.clone(),
      buffers: Mutex::new(buffers),
      device_limits: limits2.properties.limits,
      reuse_automatically
    }
  }

  pub fn get_slice(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<VkBufferSlice> {
    if info.size > BIG_BUFFER_SLAB_SIZE {
      let buffer = VkBuffer::new(&self.device, 1, memory_usage, info, &self.device.allocator, name);
      let mut free_slices = buffer.free_slices.lock().unwrap();
      let slice = free_slices.pop().unwrap();
      let mut used_slices = buffer.used_slices.lock().unwrap();
      used_slices.push(slice.clone());
      return slice;
    }

    let mut info = info.clone();
    let mut alignment: usize = 4;
    if info.usage.intersects(BufferUsage::FRAGMENT_SHADER_CONSTANT | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::COMPUTE_SHADER_CONSTANT) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_uniform_buffer_offset_alignment as usize);
    }
    if info.usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_READ)
      || info.usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_WRITE)
      || info.usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_READ)
      || info.usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE)
      || info.usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_READ)
      || info.usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_WRITE) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_storage_buffer_offset_alignment as usize);
    }

    let key = BufferKey { memory_usage, buffer_usage: info.usage };
    let mut guard = self.buffers.lock().unwrap();
    let matching_buffers = guard.entry(key).or_default();
    for buffer in matching_buffers.iter() {
      if buffer.info.size % alignment != 0 || buffer.info.size < info.size {
        continue;
      }
      let slice = {
        let mut slices = buffer.free_slices.lock().unwrap();
        slices.pop()
      };
      if slice.is_none() {
        continue;
      }
      let slice = slice.unwrap();
      let mut used_slices = buffer.used_slices.lock().unwrap();
      used_slices.push(slice.clone());
      return slice;
    }

    if self.reuse_automatically {
      for buffer in matching_buffers.iter() {
        if buffer.info.size % alignment != 0 || buffer.info.size < info.size {
          continue;
        }
        let mut used_slices = buffer.used_slices.lock().unwrap();
        let mut free_slices = buffer.free_slices.lock().unwrap();
        for slice in &*used_slices {
          if Arc::strong_count(slice) == 1 {
            free_slices.push(slice.clone());
          }
        }
        if let Some(slice) = free_slices.pop() {
          used_slices.push(slice.clone());
          return slice;
        }
      }
    }

    let mut slice_size = max(info.size, alignment);
    slice_size = if slice_size <= TINY_BUFFER_SLAB_SIZE {
      TINY_BUFFER_SLAB_SIZE
    } else if info.size <= SMALL_BUFFER_SLAB_SIZE {
      SMALL_BUFFER_SLAB_SIZE
    } else if info.size <= BUFFER_SLAB_SIZE {
      BUFFER_SLAB_SIZE
    } else {
      BIG_BUFFER_SLAB_SIZE
    };
    info.size = slice_size;

    let buffer = VkBuffer::new(&self.device, slice_size, memory_usage, &info, &self.device.allocator, None);
    let slice = {
      let mut free_slices = buffer.free_slices.lock().unwrap();
      let slice = free_slices.pop().unwrap();
      let mut used_slices = buffer.used_slices.lock().unwrap();
      used_slices.push(slice.clone());
      slice
    };
    matching_buffers.push(buffer);
    slice
  }

  pub fn reset(&self) {
    let buffers_types = self.buffers.lock().unwrap();
    for (_key, buffers) in buffers_types.iter() {
      for buffer in buffers {
        let mut used_slices = buffer.used_slices.lock().unwrap();
        let mut free_slices = buffer.free_slices.lock().unwrap();
        free_slices.append(used_slices.as_mut());
      }
    }
  }
}
