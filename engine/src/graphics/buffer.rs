use std::collections::HashMap;
use std::ffi::c_void;
use std::fmt::{
    Debug,
    Formatter,
};
use std::hash::Hash;
use std::mem::ManuallyDrop;
use std::sync::{
    Arc,
    Mutex,
};

use log::trace;

use super::gpu::{
    Buffer as _,
    Heap as _,
};
use super::*;

pub struct BufferAndAllocation {
    pub(super) buffer: active_gpu_backend::Buffer,
    pub(super) allocation: Option<MemoryAllocation<active_gpu_backend::Heap>>,
}

pub struct BufferSlice {
    buffer_allocation: ManuallyDrop<Allocation<BufferAndAllocation>>,
    destroyer: Arc<DeferredDestroyer>,
}

impl Drop for BufferSlice {
    fn drop(&mut self) {
        let buffer_allocation = unsafe { ManuallyDrop::take(&mut self.buffer_allocation) };
        self.destroyer.destroy_buffer_allocation(buffer_allocation);
    }
}

impl Debug for BufferSlice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(Buffer Slice: {}-{} (length: {}))",
            self.buffer_allocation.range.offset,
            self.buffer_allocation.range.offset + self.buffer_allocation.range.length,
            self.buffer_allocation.range.length
        )
    }
}

impl BufferSlice {
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.buffer_allocation.range.offset
    }

    #[inline(always)]
    pub fn length(&self) -> u64 {
        self.buffer_allocation.range.length
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::Buffer {
        &self.buffer_allocation.data().buffer
    }

    #[inline(always)]
    pub unsafe fn map_part(
        &self,
        offset: u64,
        length: u64,
        invalidate: bool,
    ) -> Option<*mut c_void> {
        debug_assert!(self.buffer_allocation.range.length >= offset + length);
        self.handle().map(
            self.buffer_allocation.range.offset + offset,
            length,
            invalidate,
        )
    }

    #[inline(always)]
    pub unsafe fn unmap_part(&self, offset: u64, length: u64, flush: bool) {
        debug_assert!(self.buffer_allocation.range.length >= offset + length);
        self.handle()
            .unmap(self.buffer_allocation.range.offset + offset, length, flush)
    }

    #[inline(always)]
    pub unsafe fn map(&self, invalidate: bool) -> Option<*mut c_void> {
        self.handle().map(
            self.buffer_allocation.range.offset,
            self.buffer_allocation.range.length,
            invalidate,
        )
    }

    #[inline(always)]
    pub unsafe fn unmap(&self, flush: bool) {
        self.handle().unmap(
            self.buffer_allocation.range.offset,
            self.buffer_allocation.range.length,
            flush,
        );
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

    #[inline(always)]
    pub fn info(&self) -> &BufferInfo {
        self.buffer_allocation.data().buffer.info()
    }
}

const SLICED_BUFFER_SIZE: u64 = 524288;
const UNIQUE_ALLOCATION_THRESHOLD: u64 = 65536;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
    memory_usage: MemoryUsage,
    buffer_usage: BufferUsage,
    sharing_mode: QueueSharingMode,
}

pub struct BufferAllocator {
    device: Arc<active_gpu_backend::Device>,
    allocator: Arc<MemoryAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    buffers: Mutex<HashMap<BufferKey, Vec<Chunk<BufferAndAllocation>>>>,
}

impl BufferAllocator {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        memory_allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
    ) -> Self {
        Self {
            device: device.clone(),
            allocator: memory_allocator.clone(),
            destroyer: destroyer.clone(),
            buffers: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_slice(
        &self,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        name: Option<&str>,
    ) -> Result<Arc<BufferSlice>, OutOfMemoryError> {
        let heap_info = unsafe { self.device.get_buffer_heap_info(info) };
        let alignment: u64 = heap_info.alignment;

        if info.size > UNIQUE_ALLOCATION_THRESHOLD {
            // Don't do one-off buffers for command lists
            let buffer_and_allocation = BufferAllocator::create_buffer(
                &self.device,
                &self.allocator,
                info,
                memory_usage,
                name,
            )?;
            let chunk = Chunk::new(buffer_and_allocation, info.size);
            let suballocation = chunk
                .allocate(info.size, alignment)
                .ok_or(OutOfMemoryError {})?;
            return Ok(Arc::new(BufferSlice {
                buffer_allocation: ManuallyDrop::new(suballocation),
                destroyer: self.destroyer.clone(),
            }));
        }

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
            sharing_mode: info.sharing_mode,
        };
        let mut guard = self.buffers.lock().unwrap();
        let matching_chunks = guard.entry(key).or_insert(Vec::new());

        for chunk in matching_chunks.iter() {
            if let Some(allocation) = chunk.allocate(info.size, alignment) {
                return Ok(Arc::new(BufferSlice {
                    buffer_allocation: ManuallyDrop::new(allocation),
                    destroyer: self.destroyer.clone(),
                }));
            }
        }

        let mut sliced_buffer_info = info.clone();
        sliced_buffer_info.size = SLICED_BUFFER_SIZE.max(info.size);

        let buffer_and_allocation = BufferAllocator::create_buffer(
            &self.device,
            &self.allocator,
            &sliced_buffer_info,
            memory_usage,
            None,
        )?;
        let chunk = Chunk::new(buffer_and_allocation, sliced_buffer_info.size);
        let allocation = chunk.allocate(info.size, alignment).unwrap();
        matching_chunks.push(chunk);
        return Ok(Arc::new(BufferSlice {
            buffer_allocation: ManuallyDrop::new(allocation),
            destroyer: self.destroyer.clone(),
        }));
    }

    pub(super) fn create_buffer(
        device: &Arc<active_gpu_backend::Device>,
        allocator: &MemoryAllocator,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        name: Option<&str>,
    ) -> Result<BufferAndAllocation, OutOfMemoryError> {
        let heap_info = unsafe { device.get_buffer_heap_info(info) };
        if heap_info.dedicated_allocation_preference
            == DedicatedAllocationPreference::RequireDedicated
            || heap_info.dedicated_allocation_preference
                == DedicatedAllocationPreference::PreferDedicated
        {
            let memory_types = unsafe { device.memory_type_infos() };
            let mut buffer: Result<active_gpu_backend::Buffer, OutOfMemoryError> =
                Err(OutOfMemoryError {});

            let mask = allocator
                .find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Strict)
                & heap_info.memory_type_mask;
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
                let mask = allocator
                    .find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Normal)
                    & heap_info.memory_type_mask;
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
                let mask = allocator
                    .find_memory_type_mask(memory_usage, MemoryTypeMatchingStrictness::Fallback)
                    & heap_info.memory_type_mask;
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
                allocation: None,
            })
        } else {
            let allocation = allocator.allocate(memory_usage, &heap_info)?;
            let buffer = unsafe {
                allocation.as_ref().data().create_buffer(
                    info,
                    allocation.as_ref().range.offset,
                    name,
                )
            }?;
            Ok(BufferAndAllocation {
                buffer: buffer,
                allocation: Some(allocation),
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
                trace!(
                    "Freed {} buffers in buffer type {:?}",
                    buffer_count_before - buffers.len(),
                    buffer_key
                );
            }
        }
    }
}
