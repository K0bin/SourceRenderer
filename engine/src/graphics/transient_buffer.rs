use std::{sync::Arc, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, mem::ManuallyDrop, ffi::c_void};

use sourcerenderer_core::{gpu::*, atomic_refcell::{AtomicRefCell, AtomicRefMut}};

use super::*;

pub struct TransientBufferSlice<B: GPUBackend> {
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

    pub(super) fn map(&self, invalidate: bool) -> Option<*mut c_void> {
        unsafe { self.handle().map(self.offset, self.length, invalidate) }
    }

    pub(super) fn unmap(&self, flush: bool) {
        unsafe { self.handle().unmap(self.offset, self.length, flush) }
    }
}

const BUFFER_SIZE: u64 = 16384;
const REORDER_THRESHOLD: u64 = 128;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
    memory_usage: MemoryUsage,
    buffer_usage: BufferUsage,
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
    buffers: AtomicRefCell<HashMap<BufferKey, Vec<TransientBuffer<B>>>>,
}

impl<B: GPUBackend> TransientBufferAllocator<B> {
    pub(super) fn new(
        device: &Arc<B::Device>,
        allocator: &Arc<MemoryAllocator<B>>,
        destroyer: &Arc<DeferredDestroyer<B>>
    ) -> Self {
        Self {
            device: device.clone(),
            allocator: allocator.clone(),
            destroyer: destroyer.clone(),
            buffers: AtomicRefCell::new(HashMap::new())
        }
    }

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      _name: Option<&str>,
    ) -> Result<TransientBufferSlice<B>, OutOfMemoryError> {
        let mut alignment: u64 = 256; // TODO

        let mut buffers: AtomicRefMut<'_, HashMap<BufferKey, Vec<TransientBuffer<B>>>> = self.buffers.borrow_mut();

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
        };
        let matching_buffers = buffers.entry(key).or_insert(Vec::new());

        let mut slice_opt: Option<TransientBufferSlice<B>> = None;
        let mut used_up_buffer_index: Option<usize> = None;
        for (index, sliced_buffer) in matching_buffers.iter_mut().enumerate() {
            let aligned_offset = align_up_64(sliced_buffer.offset, alignment);
            if sliced_buffer.size - aligned_offset < info.size {
                continue;
            }

            sliced_buffer.offset = aligned_offset + info.size;

            slice_opt = Some(TransientBufferSlice {
                buffer: &*sliced_buffer.buffer as *const B::Buffer,
                offset: aligned_offset,
                length: info.size
            });

            let used_up = sliced_buffer.size - sliced_buffer.offset <= REORDER_THRESHOLD;
            if used_up && index != matching_buffers.len() - 1 {
                used_up_buffer_index = Some(index);
            }
            break;
        }
        if let Some(index) = used_up_buffer_index {
            // Move now used up buffer to the end of the vector, so we don't have to iterate over it in the future
            let buffer = matching_buffers.remove(index);
            matching_buffers.push(buffer);
        }
        if let Some(slice) = slice_opt {
            return Ok(slice);
        }

        let mut new_buffer_info = info.clone();
        new_buffer_info.size = BUFFER_SIZE.max(info.size);

        let BufferAndAllocation { buffer, allocation } = BufferAllocator::create_buffer(&self.device, &self.allocator, info, memory_usage, None)?;

        let mut sliced_buffer = TransientBuffer::<B> {
            size: new_buffer_info.size,
            offset: info.size,
            buffer: ManuallyDrop::new(buffer),
            allocation,
            destroyer: self.destroyer.clone()
        };
        sliced_buffer.reset();
        let slice: TransientBufferSlice<B> = TransientBufferSlice {
            buffer: &*sliced_buffer.buffer as *const B::Buffer,
            offset: 0,
            length: info.size
        };
        matching_buffers.push(sliced_buffer);
        Ok(slice)
    }

    pub fn reset(&mut self) {
        let mut buffers: AtomicRefMut<'_, HashMap<BufferKey, Vec<TransientBuffer<B>>>> = self.buffers.borrow_mut();
        for (_key, buffers) in buffers.iter_mut() {
            for sliced_buffer in buffers.iter_mut() {
                sliced_buffer.reset();
            }
            buffers.sort_unstable_by_key(|a| a.size);
        }
    }
}
