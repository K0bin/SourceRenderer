use std::{sync::Arc, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, mem::ManuallyDrop, ffi::c_void};

use sourcerenderer_core::{gpu::*, atomic_refcell::{AtomicRefCell, AtomicRefMut}, extend_lifetime};

use super::*;

pub struct TransientBufferSlice {
    _owned_buffer: Option<Box<TransientBuffer>>,
    buffer: &'static active_gpu_backend::Buffer,
    offset: u64,
    length: u64,
    frame: u64,
}

unsafe impl Send for TransientBufferSlice {}
unsafe impl Sync for TransientBufferSlice {}

impl Debug for TransientBufferSlice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(Buffer Slice: {}-{} (length: {}))",
            self.offset,
            self.offset + self.length,
            self.length
        )
    }
}

impl TransientBufferSlice {
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    #[inline(always)]
    pub fn length(&self) -> u64 {
        self.length
    }

    #[inline(always)]
    pub(super) fn handle<'a>(&'a self, frame: u64) -> &'a active_gpu_backend::Buffer {
        assert_eq!(self.frame, frame);
        self.buffer
    }

    #[inline(always)]
    pub unsafe fn map(&self, frame: u64, invalidate: bool) -> Option<*mut c_void> {
        self.handle(frame).map(self.offset, self.length, invalidate)
    }

    #[inline(always)]
    pub unsafe fn unmap(&self, frame: u64, flush: bool) {
        self.handle(frame).unmap(self.offset, self.length, flush)
    }
}

const BUFFER_SIZE: u64 = 65536;
const BUFFER_FULL_GAP_THRESHOLD: u64 = 128;
const UNIQUE_ALLOCATION_THRESHOLD: u64 = 8192;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
    buffer_usage: BufferUsage,
    memory_usage: MemoryUsage,
    sharing_mode: QueueSharingMode
}

struct TransientBuffer {
    size: u64,
    offset: u64,
    buffer: ManuallyDrop<active_gpu_backend::Buffer>,
    allocation: Option<MemoryAllocation<active_gpu_backend::Heap>>,
    destroyer: Arc<DeferredDestroyer>
}

impl Drop for TransientBuffer {
    fn drop(&mut self) {
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        self.destroyer.destroy_buffer(buffer);
        if let Some(allocation) = self.allocation.take() {
            self.destroyer.destroy_allocation(allocation);
        }
    }
}

impl TransientBuffer {
    #[inline(always)]
    pub(crate) fn reset(&mut self) {
        self.offset = 0u64;
    }
}

pub(super) struct TransientBufferAllocator {
    device: Arc<active_gpu_backend::Device>,
    allocator: Arc<MemoryAllocator>,
    destroyer: Arc<DeferredDestroyer>,
    inner: AtomicRefCell<TransientBufferAllocatorInner>,
    is_uma: bool
}

struct BufferCollection {
    buffers: Vec<Box<TransientBuffer>>,
    first_free_index: usize
}

impl Default for BufferCollection {
    fn default() -> Self {
        Self {
            buffers: Vec::new(),
            first_free_index: 0
        }
    }
}

struct TransientBufferAllocatorInner {
    buffer_collections: HashMap<BufferKey, BufferCollection>,
    retained_size_host_memory: Option<u64>,
    retained_size_gpu_memory: Option<u64>,
}

impl TransientBufferAllocator {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        allocator: &Arc<MemoryAllocator>,
        destroyer: &Arc<DeferredDestroyer>,
        is_uma: bool
    ) -> Self {
        Self {
            device: device.clone(),
            allocator: allocator.clone(),
            destroyer: destroyer.clone(),
            inner: AtomicRefCell::new(TransientBufferAllocatorInner {
                buffer_collections: HashMap::new(),
                retained_size_host_memory: None,
                retained_size_gpu_memory: None
            }),
            is_uma
        }
    }

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      frame: u64,
      _name: Option<&str>,
    ) -> Result<TransientBufferSlice, OutOfMemoryError> {
        let heap_info = unsafe { self.device.get_buffer_heap_info(info) };
        let alignment: u64 = heap_info.alignment;

        debug_assert!(UNIQUE_ALLOCATION_THRESHOLD <= BUFFER_SIZE);

        if info.size > UNIQUE_ALLOCATION_THRESHOLD {
            // Don't do one-off buffers for command lists
            let buffer_and_alloc = BufferAllocator::create_buffer(&self.device, &self.allocator, &self.destroyer, info, memory_usage, None)?;
            let buffer = unsafe { std::ptr::read(&buffer_and_alloc.buffer as *const ManuallyDrop<active_gpu_backend::Buffer>) };
            let allocation = unsafe { std::ptr::read(&buffer_and_alloc.allocation as *const Option<MemoryAllocation<active_gpu_backend::Heap>>) };
            let destroyer = unsafe { std::ptr::read(&buffer_and_alloc.destroyer as *const Arc<DeferredDestroyer>) };
            std::mem::forget(buffer_and_alloc);
            let boxed_buffer = Box::new(TransientBuffer {
                size: info.size,
                offset: 0,
                buffer,
                allocation,
                destroyer,
            });
            let boxed_buffer_ref: &'static active_gpu_backend::Buffer = unsafe { extend_lifetime(&boxed_buffer.as_ref().buffer) };
            let slice = TransientBufferSlice {
                _owned_buffer: Some(boxed_buffer),
                buffer: boxed_buffer_ref,
                offset: 0,
                length: info.size,
                frame,
            };
            return Ok(slice);
        }

        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner> = self.inner.borrow_mut();
        let buffers = &mut inner.buffer_collections;

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
            sharing_mode: info.sharing_mode
        };
        let matching_buffers = buffers.entry(key).or_default();

        let mut slice_opt: Option<TransientBufferSlice> = None;
        for (index, sliced_buffer) in (&mut matching_buffers.buffers[matching_buffers.first_free_index..]).iter_mut().enumerate() {
            let actual_index = index + matching_buffers.first_free_index;
            let aligned_offset = align_up_64(sliced_buffer.offset, alignment);
            let alignment_diff = aligned_offset - sliced_buffer.offset;
            if sliced_buffer.size < info.size + alignment_diff {
                continue;
            }

            sliced_buffer.offset = aligned_offset + info.size;

            slice_opt = Some(TransientBufferSlice {
                _owned_buffer: None,
                buffer: unsafe { std::mem::transmute(&sliced_buffer.buffer) },
                offset: aligned_offset,
                length: info.size,
                frame,
            });

            let used_up = sliced_buffer.size - sliced_buffer.offset <= BUFFER_FULL_GAP_THRESHOLD;
            if used_up && actual_index != matching_buffers.buffers.len() - 1 {
                matching_buffers.first_free_index = actual_index + 1;
            }
            break;
        }
        if let Some(slice) = slice_opt {
            return Ok(slice);
        }

        let mut new_buffer_info = info.clone();
        new_buffer_info.size = BUFFER_SIZE.max(info.size);

        let buffer_and_alloc = BufferAllocator::create_buffer(&self.device, &self.allocator, &self.destroyer, &new_buffer_info, memory_usage, None)?;
        let buffer = unsafe { std::ptr::read(&buffer_and_alloc.buffer as *const ManuallyDrop<active_gpu_backend::Buffer>) };
        let allocation = unsafe { std::ptr::read(&buffer_and_alloc.allocation as *const Option<MemoryAllocation<active_gpu_backend::Heap>>) };
        let destroyer = unsafe { std::ptr::read(&buffer_and_alloc.destroyer as *const Arc<DeferredDestroyer>) };
        std::mem::forget(buffer_and_alloc);

        let mut sliced_buffer = Box::new(TransientBuffer {
            size: new_buffer_info.size,
            offset: 0,
            buffer: buffer,
            allocation,
            destroyer,
        });
        sliced_buffer.reset();
        let slice: TransientBufferSlice = TransientBufferSlice {
            _owned_buffer: None,
            buffer: unsafe { extend_lifetime(&sliced_buffer.buffer) },
            offset: 0,
            length: info.size,
            frame,
        };
        sliced_buffer.offset += info.size;
        matching_buffers.buffers.push(sliced_buffer);
        Ok(slice)
    }

    pub fn reset(&self) {
        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner> = self.inner.borrow_mut();
        let retained_gpu_memory = inner.retained_size_gpu_memory.unwrap_or(u64::MAX);
        let retained_host_memory = inner.retained_size_host_memory.unwrap_or(u64::MAX);
        let mut counted_gpu_memory = 0u64;
        let mut counted_host_memory = 0u64;

        if retained_gpu_memory != u64::MAX || retained_host_memory != u64::MAX {
            for (key, buffer_collections) in &mut inner.buffer_collections {
                if key.buffer_usage.contains(BufferUsage::CONSTANT) {
                    // Keep constant buffers around.
                    continue;
                }

                let (counted_memory, limit) = if self.is_uma {
                    (&mut counted_host_memory, retained_host_memory)
                } else if key.memory_usage == MemoryUsage::GPUMemory || key.memory_usage == MemoryUsage::MappableGPUMemory {
                    (&mut counted_gpu_memory, retained_gpu_memory)
                } else {
                    (&mut counted_host_memory, retained_host_memory)
                };

                buffer_collections.buffers.retain(|buffer| {
                    *counted_memory += buffer.buffer.info().size;
                    *counted_memory < limit
                });
            }
        }

        for (_key, buffer_collection) in inner.buffer_collections.iter_mut() {
            for sliced_buffer in buffer_collection.buffers.iter_mut() {
                sliced_buffer.reset();
            }
            buffer_collection.buffers.sort_unstable_by_key(|a| a.size);
            buffer_collection.first_free_index = 0;
        }
    }

    #[allow(unused)]
    pub fn set_retained_size(&mut self, host_size: Option<u64>, gpu_size: Option<u64>) {
        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner> = self.inner.borrow_mut();
        inner.retained_size_host_memory = host_size;
        inner.retained_size_gpu_memory = gpu_size;
    }
}
