use std::{sync::{Arc, Mutex}, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, ffi::c_void};

use log::trace;
use sourcerenderer_core::gpu::*;

use super::*;

pub struct BufferAndAllocation<B: GPUBackend> {
    pub(super) buffer: B::Buffer,
    pub(super) allocation: Option<MemoryAllocation<B::Heap>>
}

pub struct BufferSlice<B: GPUBackend>(Allocation<BufferAndAllocation<B>>);

impl<B: GPUBackend> Debug for BufferSlice<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(Buffer Slice: {}-{} (length: {}))",
            self.0.range.offset,
            self.0.range.offset + self.0.range.length,
            self.0.range.length
        )
    }
}

impl<B: GPUBackend> BufferSlice<B> {
    pub fn offset(&self) -> u64 {
        self.0.range.offset
    }

    pub fn length(&self) -> u64 {
        self.0.range.length
    }

    pub(super) fn handle(&self) -> &B::Buffer {
        &self.0.data().buffer
    }

    pub unsafe fn map_part(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void> {
        debug_assert!(self.0.range.length >= offset + length);
        self.handle().map(self.0.range.offset + offset, length, invalidate)
    }

    pub unsafe fn unmap_part(&self, offset: u64, length: u64, flush: bool) {
        debug_assert!(self.0.range.length >= offset + length);
        self.handle().unmap(self.0.range.offset + offset, length, flush)
    }

    pub unsafe fn map(&self, invalidate: bool) -> Option<*mut c_void> {
        self.handle().map(self.0.range.offset, self.0.range.length, invalidate)
    }

    pub unsafe fn unmap(&self, flush: bool) {
        self.handle().unmap(self.0.range.offset, self.0.range.length, flush);
    }

    pub fn write<T: Clone>(&self, src: &T) -> Option<()> {
        unsafe {
            let ptr_opt = self.map(false);
            if ptr_opt.is_none() {
                return None;
            }
            let ptr = ptr_opt.unwrap();
            std::ptr::copy(src, std::mem::transmute(ptr), 1);
            self.unmap(true);
            return Some(());
        }
    }

    pub fn info(&self) -> &BufferInfo {
        self.0.data().buffer.info()
    }
}

const SLICED_BUFFER_SIZE: u64 = 524288;
const UNIQUE_ALLOCATION_THRESHOLD: u64 = 65536;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
    memory_usage: MemoryUsage,
    buffer_usage: BufferUsage,
    sharing_mode: QueueSharingMode
}

pub struct BufferAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    allocator: Arc<MemoryAllocator<B>>,
    buffers: Mutex<HashMap<BufferKey, Vec<Chunk<BufferAndAllocation<B>>>>>,
}

impl<B: GPUBackend> BufferAllocator<B> {
    pub(super) fn new(device: &Arc<B::Device>, memory_allocator: &Arc<MemoryAllocator<B>>) -> Self {
        Self {
            device: device.clone(),
            allocator: memory_allocator.clone(),
            buffers: Mutex::new(HashMap::new())
        }
    }

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      name: Option<&str>,
    ) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        let heap_info = unsafe { self.device.get_buffer_heap_info(info) };
        let alignment: u64 = heap_info.alignment;

        if info.size > UNIQUE_ALLOCATION_THRESHOLD {
            // Don't do one-off buffers for command lists
            let buffer_and_allocation = BufferAllocator::create_buffer(&self.device, &self.allocator, info, memory_usage, name)?;
            let chunk = Chunk::new(buffer_and_allocation, info.size);
            let suballocation = chunk.allocate(info.size, alignment).ok_or(OutOfMemoryError {})?;
            return Ok(Arc::new(BufferSlice(suballocation)));
        }

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
            sharing_mode: info.sharing_mode
        };
        let mut guard = self.buffers.lock().unwrap();
        let matching_chunks = guard.entry(key).or_insert(Vec::new());

        for chunk in matching_chunks.iter() {
            if let Some(allocation) = chunk.allocate(info.size, alignment) {
                return Ok(Arc::new(BufferSlice(allocation)));
            }
        }

        let mut sliced_buffer_info = info.clone();
        sliced_buffer_info.size = SLICED_BUFFER_SIZE.max(info.size);

        let buffer_and_allocation = BufferAllocator::create_buffer(&self.device, &self.allocator, &sliced_buffer_info, memory_usage, None)?;
        let chunk = Chunk::new(buffer_and_allocation, sliced_buffer_info.size);
        let allocation = chunk.allocate(info.size, alignment).unwrap();
        matching_chunks.push(chunk);
        return Ok(Arc::new(BufferSlice(allocation)));
    }

    pub(super) fn create_buffer(device: &Arc<B::Device>, allocator: &MemoryAllocator<B>, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Result<BufferAndAllocation<B>, OutOfMemoryError> {
        let heap_info = unsafe { device.get_buffer_heap_info(info) };
        if heap_info.dedicated_allocation_preference == DedicatedAllocationPreference::RequireDedicated || heap_info.dedicated_allocation_preference == DedicatedAllocationPreference::PreferDedicated {
            let memory_types = unsafe { device.memory_type_infos() };
            let mut buffer: Result<B::Buffer, OutOfMemoryError> = Err(OutOfMemoryError {});

            let mask = allocator.find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Strict) & heap_info.memory_type_mask;
            for i in 0..memory_types.len() as u32 {
                if (mask & (1 << i)) == 0 {
                    continue;
                }
                buffer = unsafe { device.create_buffer(info, i, name) };
                if buffer.is_ok() {
                    break;
                }
            }

            if buffer.is_err() {
                let mask = allocator.find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Normal) & heap_info.memory_type_mask;
                for i in 0..memory_types.len() as u32 {
                    if (mask & (1 << i)) == 0 {
                        continue;
                    }
                    buffer = unsafe { device.create_buffer(info, i, name) };
                    if buffer.is_ok() {
                        break;
                    }
                }
            }

            if buffer.is_err() {
                let mask = allocator.find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Fallback) & heap_info.memory_type_mask;
                for i in 0..memory_types.len() as u32 {
                    if (mask & (1 << i)) == 0 {
                        continue;
                    }
                    buffer = unsafe { device.create_buffer(info, i, name) };
                    if buffer.is_ok() {
                        break;
                    }
                }
            }

            Ok(BufferAndAllocation {
                buffer: buffer?,
                allocation: None
            })
        } else {
            let allocation = allocator.allocate(memory_usage, &heap_info)?;
            let buffer = unsafe { allocation.as_ref().data().create_buffer(info, allocation.as_ref().range.offset, name) }?;
            Ok(BufferAndAllocation {
                buffer,
                allocation: Some(allocation)
            })
        }
    }

    pub fn cleanup_unused(&self) {
        let mut guard = self.buffers.lock().unwrap();
        for (buffer_key, buffers) in guard.iter_mut() {
            let mut retained_empty = 0u32;
            let buffer_count_before = buffers.len();
            buffers.retain(|b| {
                if !b.is_empty() {
                    return true;
                }
                retained_empty += 1;
                retained_empty < 2
            });
            if buffers.len() != buffer_count_before {
                trace!("Freed {} buffers in buffer type {:?}", buffer_count_before - buffers.len(), buffer_key);
            }
        }
    }
}
