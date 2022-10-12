use std::{collections::HashMap, fmt::Formatter, sync::{Arc, Mutex}, mem::MaybeUninit};
use std::cmp::max;
use std::hash::{Hash, Hasher};
use std::fmt::Debug;
use std::ffi::CString;

use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use ash::vk::{self, BufferDeviceAddressInfo, Handle, SharingMode};

use crate::raw::*;
use crate::device::memory_usage_to_vma;
use smallvec::SmallVec;
pub struct VkBuffer {
  buffer: vk::Buffer,
  allocation: vma_sys::VmaAllocation,
  device: Arc<RawVkDevice>,
  map_ptr: Option<*mut u8>,
  memory_usage: MemoryUsage,
  info: BufferInfo,
  va: Option<vk::DeviceSize>
}

unsafe impl Send for VkBuffer {}
unsafe impl Sync for VkBuffer {}

impl VkBuffer {
  pub fn new(device: &Arc<RawVkDevice>, memory_usage: MemoryUsage, info: &BufferInfo, allocator: &vma_sys::VmaAllocator, pool: Option<vma_sys::VmaPool>, name: Option<&str>) -> Arc<Self> {
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
      size: info.size as u64,
      usage: buffer_usage_to_vk(info.usage, device.features.contains(VkFeatures::RAY_TRACING)),
      sharing_mode,
      p_queue_family_indices: queue_families.as_ptr(),
      queue_family_index_count: queue_families.len() as u32,
      ..Default::default()
    };
    let vk_mem_flags = memory_usage_to_vma(memory_usage);
    let allocation_create_info = vma_sys::VmaAllocationCreateInfo {
      flags: if memory_usage != MemoryUsage::VRAM { vma_sys::VmaAllocationCreateFlagBits_VMA_ALLOCATION_CREATE_MAPPED_BIT as u32 } else { 0 },
      usage: vma_sys::VmaMemoryUsage_VMA_MEMORY_USAGE_UNKNOWN,
      preferredFlags: vk_mem_flags.preferred,
      requiredFlags: vk_mem_flags.required,
      memoryTypeBits: 0,
      pool: pool.unwrap_or(std::ptr::null_mut()),
      pUserData: std::ptr::null_mut(),
      priority: 0f32
    };
    let mut buffer: vk::Buffer = vk::Buffer::null();
    let mut allocation: vma_sys::VmaAllocation = std::ptr::null_mut();
    let mut allocation_info_uninit: MaybeUninit<vma_sys::VmaAllocationInfo> = MaybeUninit::uninit();
    let allocation_info: vma_sys::VmaAllocationInfo;
    unsafe {
      assert_eq!(vma_sys::vmaCreateBuffer(*allocator, &buffer_info, &allocation_create_info, &mut buffer, &mut allocation, allocation_info_uninit.as_mut_ptr()), vk::Result::SUCCESS);
      allocation_info = allocation_info_uninit.assume_init();
    };

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

    let map_ptr: Option<*mut u8> = unsafe {
      if memory_usage != MemoryUsage::VRAM && allocation_info.pMappedData != std::ptr::null_mut() {
        Some(std::mem::transmute(allocation_info.pMappedData))
      } else {
        None
      }
    };

    let va = if buffer_info.usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
      device.rt.as_ref().map(|rt| {
        unsafe {
          rt.bda.get_buffer_device_address(&BufferDeviceAddressInfo {
            buffer,
            ..Default::default()
          })
        }
      })
    } else {
      None
    };

    Arc::new(VkBuffer {
      buffer,
      allocation,
      device: device.clone(),
      map_ptr,
      memory_usage,
      info: info.clone(),
      va
    })
  }

  pub fn handle(&self) -> &vk::Buffer {
    &self.buffer
  }

  pub fn va(&self) -> Option<vk::DeviceAddress> {
    self.va
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
    if !invalidate {
      let allocator = self.buffer.device.allocator;
      assert_eq!(vma_sys::vmaInvalidateAllocation(allocator, self.buffer.allocation, self.offset as u64, self.length as u64), vk::Result::SUCCESS);
    }
    self.buffer.map_ptr.map(|ptr| ptr.add(self.offset()))
  }

  unsafe fn unmap_unsafe(&self, flush: bool) {
    if !flush {
      return;
    }
    let allocator = self.buffer.device.allocator;
    assert_eq!(vma_sys::vmaFlushAllocation(allocator, self.buffer.allocation, self.offset as u64, self.length as u64), vk::Result::SUCCESS);
  }

  fn length(&self) -> usize {
    self.length
  }

  fn info(&self) -> &BufferInfo {
    &self.buffer.info
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
    return value
  }
  if value == 0 {
    return 0
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down(value: usize, alignment: usize) -> usize {
  if alignment == 0 {
    return value
  }
  (value / alignment) * alignment
}

pub(crate) fn align_up_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
    return value
  }
  if value == 0 {
    return 0
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
    return value
  }
  (value / alignment) * alignment
}

pub(crate) fn align_up_64(value: u64, alignment: u64) -> u64 {
  if alignment == 0 {
    return value
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_64(value: u64, alignment: u64) -> u64 {
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
  pub fn buffer(&self) -> &Arc<VkBuffer> {
    &self.buffer
  }

  pub fn offset(&self) -> usize {
    self.offset
  }

  pub fn length(&self) -> usize {
    self.length
  }

  pub fn va(&self) -> Option<vk::DeviceAddress> {
    self.buffer.va().map(|va| va + self.offset as vk::DeviceSize)
  }

  pub fn va_offset(&self, offset: usize) -> Option<vk::DeviceAddress> {
    self.buffer.va().map(|va| va + (self.offset + offset) as vk::DeviceSize)
  }
}

const SLICED_BUFFER_SIZE: usize = 16384;
const BIG_BUFFER_SLAB_SIZE: usize = 4096;
const BUFFER_SLAB_SIZE: usize = 1024;
const SMALL_BUFFER_SLAB_SIZE: usize = 512;
const TINY_BUFFER_SLAB_SIZE: usize = 256;
const STAGING_BUFFER_POOL_SIZE: usize = 16 << 20;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
  memory_usage: MemoryUsage,
  buffer_usage: BufferUsage
}

#[derive(Default)]
struct VkBufferSliceCollection {
  used_slices: Vec<Arc<VkBufferSlice>>,
  free_slices: Vec<Arc<VkBufferSlice>>
}

pub struct BufferAllocator {
  device: Arc<RawVkDevice>,
  buffers: Mutex<HashMap<BufferKey, VkBufferSliceCollection>>,
  device_limits: vk::PhysicalDeviceLimits,
  reuse_automatically: bool,
  transfer_pool: Option<vma_sys::VmaPool>,
}

unsafe impl Send for BufferAllocator {}
unsafe impl Sync for BufferAllocator {}

impl BufferAllocator {
  pub fn new(device: &Arc<RawVkDevice>, reuse_automatically: bool) -> Self {
    let buffers: HashMap<BufferKey, VkBufferSliceCollection> = HashMap::new();
    let mut limits2 = vk::PhysicalDeviceProperties2 {
      ..Default::default()
    };

    unsafe {
      device.instance.get_physical_device_properties2(device.physical_device, &mut limits2)
    }

    // Pure copy buffers are expected to be very short lived, so put them into a separate pool to avoid
    // fragmentation
    let transfer_pool = if reuse_automatically {
      let buffer_info = vk::BufferCreateInfo {
        size: 1024,
        usage: buffer_usage_to_vk(BufferUsage::COPY_SRC, false),
        sharing_mode: SharingMode::EXCLUSIVE,
        queue_family_index_count: 0,
        p_queue_family_indices: std::ptr::null(),
        ..Default::default()
      };
      let vk_mem_flags = memory_usage_to_vma(MemoryUsage::UncachedRAM);
      let allocation_info = vma_sys::VmaAllocationCreateInfo {
        flags: vma_sys::VmaAllocationCreateFlagBits_VMA_ALLOCATION_CREATE_MAPPED_BIT as u32,
        usage: vma_sys::VmaMemoryUsage_VMA_MEMORY_USAGE_UNKNOWN,
        preferredFlags: vk_mem_flags.preferred,
        requiredFlags: vk_mem_flags.required,
        memoryTypeBits: 0,
        pool: std::ptr::null_mut(),
        pUserData: std::ptr::null_mut(),
        priority: 0f32
      };
      let mut memory_type_index: u32 = 0;
      unsafe {
        assert_eq!(vma_sys::vmaFindMemoryTypeIndexForBufferInfo(device.allocator, &buffer_info as *const vk::BufferCreateInfo, &allocation_info as *const vma_sys::VmaAllocationCreateInfo, &mut memory_type_index as *mut u32), vk::Result::SUCCESS);
      }

      let pool_info = vma_sys::VmaPoolCreateInfo {
        memoryTypeIndex: memory_type_index,
        flags: 0,
        blockSize: STAGING_BUFFER_POOL_SIZE as vk::DeviceSize,
        minBlockCount: 0,
        maxBlockCount: 0,
        priority: 0.1f32,
        minAllocationAlignment: 0,
        pMemoryAllocateNext: std::ptr::null_mut(),
      };
      unsafe {
        let mut pool: vma_sys::VmaPool = std::ptr::null_mut();
        let res = vma_sys::vmaCreatePool(device.allocator, &pool_info as *const vma_sys::VmaPoolCreateInfo, &mut pool as *mut vma_sys::VmaPool);
        if res != vk::Result::SUCCESS {
          None
        } else {
          Some(pool)
        }
      }
    } else {
      None
    };

    BufferAllocator {
      device: device.clone(),
      buffers: Mutex::new(buffers),
      device_limits: limits2.properties.limits,
      reuse_automatically,
      transfer_pool,
    }
  }

  pub fn get_slice(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<VkBufferSlice> {
    if info.size > BIG_BUFFER_SLAB_SIZE && self.reuse_automatically {
      let pool = if memory_usage == MemoryUsage::UncachedRAM && info.usage == BufferUsage::COPY_SRC {
        self.transfer_pool
      } else {
        None
      };

      // Don't do one-off buffers for command lists
      let buffer = VkBuffer::new(&self.device, memory_usage, info, &self.device.allocator, pool, name);
      return Arc::new(VkBufferSlice {
        buffer,
        offset: 0,
        length: info.size
      });
    }

    let mut info = info.clone();
    let mut alignment: usize = 4;
    if info.usage.contains(BufferUsage::CONSTANT) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_uniform_buffer_offset_alignment as usize);
    }
    if info.usage.contains(BufferUsage::STORAGE) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, self.device_limits.min_storage_buffer_offset_alignment as usize);
    }
    if info.usage.contains(BufferUsage::ACCELERATION_STRUCTURE) {
      // TODO max doesnt guarantee both alignments
      alignment = max(alignment, 256);
    }
    if info.usage.contains(BufferUsage::SHADER_BINDING_TABLE) {
      let rt = self.device.rt.as_ref().unwrap();
      alignment = max(alignment, rt.rt_pipeline_properties.shader_group_handle_alignment as usize);
      alignment = max(alignment, rt.rt_pipeline_properties.shader_group_base_alignment as usize);
    }

    let key = BufferKey { memory_usage, buffer_usage: info.usage };
    let mut guard = self.buffers.lock().unwrap();
    let matching_buffers = guard.entry(key).or_default();
    // TODO: consider a smarter data structure than a simple list of all slices regardless of size.
    let slice_index = matching_buffers.free_slices.iter()
      .enumerate()
      .find(|(_, slice)| slice.offset % alignment == 0 && slice.length % alignment == 0 && slice.length >= info.size)
      .map(|(index, _b)| index);
    if let Some(index) = slice_index {
      let slice = matching_buffers.free_slices.remove(index);
      matching_buffers.used_slices.push(slice.clone());
      return slice;
    }

    if self.reuse_automatically && !matching_buffers.used_slices.is_empty() {
      // This is awful. Completely rewrite this with drain_filter once that's stabilized.
      // Right now cleaner alternatives would likely need to do more copying and allocations.
      let length = matching_buffers.used_slices.len();
      for i in (0..length).rev() {
        let refcount = {
          let slice = &matching_buffers.used_slices[i];
          Arc::strong_count(slice)
        };
        if refcount == 1 {
          matching_buffers.free_slices.push(matching_buffers.used_slices.remove(i));
        }
      }
      let slice_index = matching_buffers.free_slices.iter()
        .enumerate()
        .find(|(_, slice)| slice.offset % alignment == 0 && slice.length % alignment == 0 && slice.length >= info.size)
        .map(|(index, _b)| index);
      if let Some(index) = slice_index {
        let slice = matching_buffers.free_slices.remove(index);
        matching_buffers.used_slices.push(slice.clone());
        return slice;
      }
    }

    let mut slice_size = align_up(info.size, alignment);
    slice_size = if slice_size <= TINY_BUFFER_SLAB_SIZE {
      TINY_BUFFER_SLAB_SIZE
    } else if info.size <= SMALL_BUFFER_SLAB_SIZE {
      SMALL_BUFFER_SLAB_SIZE
    } else if info.size <= BUFFER_SLAB_SIZE {
      BUFFER_SLAB_SIZE
    } else if info.size <= BIG_BUFFER_SLAB_SIZE {
      BIG_BUFFER_SLAB_SIZE
    } else {
      info.size
    };
    let slices = if slice_size <= BIG_BUFFER_SLAB_SIZE {
      info.size = SLICED_BUFFER_SIZE;
      SLICED_BUFFER_SIZE / slice_size
    } else {
      1
    };

    let buffer = VkBuffer::new(&self.device, memory_usage, &info, &self.device.allocator, None, None);
    for i in 0 .. (slices - 1) {
      let slice = Arc::new(VkBufferSlice {
        buffer: buffer.clone(),
        offset: i * slice_size,
        length: slice_size
      });
      matching_buffers.free_slices.push(slice);
    }
    let slice = Arc::new(VkBufferSlice {
      buffer,
      offset: (slices - 1) * slice_size,
      length: slice_size
    });
    matching_buffers.used_slices.push(slice.clone());
    slice
  }

  pub fn reset(&self) {
    let mut buffers_types = self.buffers.lock().unwrap();
    for (_key, buffers) in buffers_types.iter_mut() {
      buffers.free_slices.append(buffers.used_slices.as_mut());
    }
  }
}

impl Drop for BufferAllocator {
  fn drop(&mut self) {
    if let Some(pool) = self.transfer_pool {
      unsafe {
        vma_sys::vmaDestroyPool(self.device.allocator, pool);
      }
    }
  }
}
