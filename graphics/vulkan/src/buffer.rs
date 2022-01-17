use std::{collections::HashMap, fmt::Formatter, sync::{Arc, Mutex}};
use std::cmp::max;
use std::hash::{Hash, Hasher};
use std::fmt::Debug;
use std::ffi::CString;

use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use ash::vk;
use ash::vk::Handle;

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
    self.device.allocator.destroy_buffer(self.buffer, &self.allocation);
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
    if invalidate && !self.buffer.is_coherent &&
      (self.buffer.memory_usage == MemoryUsage::CpuToGpu
        || self.buffer.memory_usage == MemoryUsage::CpuOnly
        || self.buffer.memory_usage == MemoryUsage::GpuToCpu) {
      let allocator = &self.buffer.device.allocator;
      allocator.invalidate_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length);
    }
    self.buffer.map_ptr.map(|ptr| ptr.add(self.offset))
  }

  unsafe fn unmap_unsafe(&self, flush: bool) {
    if flush && !self.buffer.is_coherent &&
      (self.buffer.memory_usage == MemoryUsage::CpuToGpu
        || self.buffer.memory_usage == MemoryUsage::CpuOnly
        || self.buffer.memory_usage == MemoryUsage::GpuToCpu) {
      let allocator = &self.buffer.device.allocator;
      allocator.flush_allocation(&self.buffer.allocation, self.buffer.allocation_info.get_offset() + self.offset, self.length);
    }
  }

  fn get_length(&self) -> usize {
    self.length
  }

  fn get_info(&self) -> &BufferInfo {
    &self.buffer.info
  }
}

pub fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
  let mut flags = vk::BufferUsageFlags::empty();

  if usage.contains(BufferUsage::STORAGE) {
    flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
  }

  if usage.contains(BufferUsage::CONSTANT) {
    flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
  }

  if usage.contains(BufferUsage::VERTEX) {
    flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
  }

  if usage.contains(BufferUsage::INDEX) {
    flags |= vk::BufferUsageFlags::INDEX_BUFFER;
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

  flags
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
    if info.usage.contains(BufferUsage::CONSTANT) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_uniform_buffer_offset_alignment as usize);
    }
    if info.usage.contains(BufferUsage::STORAGE){
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
        // This is awful. Completely rewrite this with drain_filter once that's stabilized.
        // Right now cleaner alternatives would likely need to do more copying and allocations.
        let mut i: isize = (used_slices.len() - 1) as isize;
        while i >= 0 {
          let index = i as usize;
          let refcount = {
            let slice = &used_slices[index];
            Arc::strong_count(slice)
          };
          if refcount == 1 {
            free_slices.push(used_slices.remove(index));
            i -= 1;
          }
          i -= 1;
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
