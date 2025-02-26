use std::{sync::Arc, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, mem::ManuallyDrop, ffi::c_void};

use sourcerenderer_core::{gpu::*, atomic_refcell::{AtomicRefCell, AtomicRefMut}};

use super::*;

pub struct TransientBufferSlice<B: GPUBackend> {
    owned_buffer: Option<Box<TransientBuffer<B>>>,
    buffer: *const B::Buffer,
    offset: u64,
    length: u64
}

unsafe impl<B: GPUBackend> Send for TransientBufferSlice<B> {}
unsafe impl<B: GPUBackend> Sync for TransientBufferSlice<B> {}

impl<B: GPUBackend> Debug for TransientBufferSlice<B> {
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

impl<B: GPUBackend> TransientBufferSlice<B> {
    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub(super) fn handle(&self) -> &B::Buffer {
        unsafe { &*self.buffer }
    }

    pub unsafe fn map(&self, invalidate: bool) -> Option<*mut c_void> {
        self.handle().map(self.offset, self.length, invalidate)
    }

    pub unsafe fn unmap(&self, flush: bool) {
        self.handle().unmap(self.offset, self.length, flush)
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

struct TransientBuffer<B: GPUBackend> {
    size: u64,
    offset: u64,
    buffer: ManuallyDrop<B::Buffer>,
    allocation: Option<MemoryAllocation<B::Heap>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for TransientBuffer<B> {
    fn drop(&mut self) {
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        self.destroyer.destroy_buffer(buffer);
        if let Some(allocation) = self.allocation.take() {
            self.destroyer.destroy_allocation(allocation);
        }
    }
}

impl<B: GPUBackend> TransientBuffer<B> {
    pub(crate) fn reset(&mut self) {
        self.offset = 0u64;
    }
}

pub(super) struct TransientBufferAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    allocator: Arc<MemoryAllocator<B>>,
    destroyer: Arc<DeferredDestroyer<B>>,
    inner: AtomicRefCell<TransientBufferAllocatorInner<B>>,
    is_uma: bool
}

struct BufferCollection<B: GPUBackend> {
    buffers: Vec<Box<TransientBuffer<B>>>,
    first_free_index: usize
}

impl<B: GPUBackend> Default for BufferCollection<B> {
    fn default() -> Self {
        Self {
            buffers: Vec::new(),
            first_free_index: 0
        }
    }
}

struct TransientBufferAllocatorInner<B: GPUBackend> {
    buffer_collections: HashMap<BufferKey, BufferCollection<B>>,
    retained_size_host_memory: Option<u64>,
    retained_size_gpu_memory: Option<u64>,
}

impl<B: GPUBackend> TransientBufferAllocator<B> {
    pub(super) fn new(
        device: &Arc<B::Device>,
        allocator: &Arc<MemoryAllocator<B>>,
        destroyer: &Arc<DeferredDestroyer<B>>,
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
      _name: Option<&str>,
    ) -> Result<TransientBufferSlice<B>, OutOfMemoryError> {
        let heap_info = unsafe { self.device.get_buffer_heap_info(info) };
        let alignment: u64 = heap_info.alignment;

        debug_assert!(UNIQUE_ALLOCATION_THRESHOLD <= BUFFER_SIZE);

        if info.size > UNIQUE_ALLOCATION_THRESHOLD {
            // Don't do one-off buffers for command lists
            let BufferAndAllocation { buffer, allocation } = BufferAllocator::create_buffer(&self.device, &self.allocator, info, memory_usage, None)?;
            let mut slice = TransientBufferSlice {
                owned_buffer: Some(Box::new(TransientBuffer {
                    size: info.size,
                    offset: 0,
                    buffer: ManuallyDrop::new(buffer),
                    allocation,
                    destroyer: self.destroyer.clone()
                })),
                buffer: std::ptr::null(),
                offset: 0,
                length: info.size
            };
            slice.buffer = &*slice.owned_buffer.as_ref().unwrap().buffer as *const B::Buffer;
            return Ok(slice);
        }

        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner<B>> = self.inner.borrow_mut();
        let buffers = &mut inner.buffer_collections;

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
            sharing_mode: info.sharing_mode
        };
        let matching_buffers = buffers.entry(key).or_default();

        let mut slice_opt: Option<TransientBufferSlice<B>> = None;
        for (index, sliced_buffer) in (&mut matching_buffers.buffers[matching_buffers.first_free_index..]).iter_mut().enumerate() {
            let actual_index = index + matching_buffers.first_free_index;
            let aligned_offset = align_up_64(sliced_buffer.offset, alignment);
            let alignment_diff = aligned_offset - sliced_buffer.offset;
            if sliced_buffer.size < info.size + alignment_diff {
                continue;
            }

            sliced_buffer.offset = aligned_offset + info.size;

            slice_opt = Some(TransientBufferSlice {
                owned_buffer: None,
                buffer: &*sliced_buffer.buffer as *const B::Buffer,
                offset: aligned_offset,
                length: info.size
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

        let BufferAndAllocation { buffer, allocation } = BufferAllocator::create_buffer(&self.device, &self.allocator, &new_buffer_info, memory_usage, None)?;

        let mut sliced_buffer = Box::new(TransientBuffer::<B> {
            size: new_buffer_info.size,
            offset: 0,
            buffer: ManuallyDrop::new(buffer),
            allocation,
            destroyer: self.destroyer.clone()
        });
        sliced_buffer.reset();
        let slice: TransientBufferSlice<B> = TransientBufferSlice {
            owned_buffer: None,
            buffer: &*sliced_buffer.buffer as *const B::Buffer,
            offset: 0,
            length: info.size
        };
        sliced_buffer.offset += info.size;
        matching_buffers.buffers.push(sliced_buffer);
        Ok(slice)
    }

    pub fn reset(&self) {
        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner<B>> = self.inner.borrow_mut();
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

    pub fn set_retained_size(&mut self, host_size: Option<u64>, gpu_size: Option<u64>) {
        let mut inner: AtomicRefMut<'_, TransientBufferAllocatorInner<B>> = self.inner.borrow_mut();
        inner.retained_size_host_memory = host_size;
        inner.retained_size_gpu_memory = gpu_size;
    }
}
