use std::{sync::{Arc, Mutex}, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, mem::ManuallyDrop};

use sourcerenderer_core::gpu::*;

use super::*;

pub struct BufferSlice<B: GPUBackend> {
  buffer: ManuallyDrop<Arc<B::Buffer>>,
  destroyer: Arc<DeferredDestroyer<B>>,
  offset: u64,
  length: u64
}

impl<B: GPUBackend> Debug for BufferSlice<B> {
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

impl<B: GPUBackend> Drop for BufferSlice<B> {
    fn drop(&mut self) {
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        self.destroyer.destroy_buffer_reference(buffer);
    }
}

const SLICED_BUFFER_SIZE: u64 = 16384;
const BIG_BUFFER_SLAB_SIZE: u64 = 4096;
const BUFFER_SLAB_SIZE: u64 = 1024;
const SMALL_BUFFER_SLAB_SIZE: u64 = 512;
const TINY_BUFFER_SLAB_SIZE: u64 = 256;
const STAGING_BUFFER_POOL_SIZE: u64 = 16 << 20;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct BufferKey {
    memory_usage: MemoryUsage,
    buffer_usage: BufferUsage,
}

struct SlicedBuffer<B: GPUBackend> {
    slice_size: u64,
    buffer: ManuallyDrop<Arc<B::Buffer>>,
    free_slices: Vec<BufferSlice<B>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for SlicedBuffer<B> {
    fn drop(&mut self) {
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        self.destroyer.destroy_buffer_reference(buffer);
    }
}

impl<B: GPUBackend> SlicedBuffer<B> {
    pub(crate) fn reset(&mut self) {
        let slices = (SLICED_BUFFER_SIZE / self.slice_size).max(1);
        for i in 0..slices {
          let slice: BufferSlice<_> = BufferSlice {
                buffer: self.buffer.clone(),
                destroyer: self.destroyer.clone(),
                offset: i * self.slice_size,
                length: self.slice_size
          };
          self.free_slices.push(slice);
        }
    }
}

pub struct BufferAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    buffers: Mutex<HashMap<BufferKey, Vec<SlicedBuffer<B>>>>,
}

impl<B: GPUBackend> BufferAllocator<B> {

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      name: Option<&str>,
    ) -> BufferSlice<B> {

        if info.size > BIG_BUFFER_SLAB_SIZE {
            // Don't do one-off buffers for command lists
            let buffer = unsafe {
                Arc::new(self.device.create_buffer(info, memory_usage, name))
            };
            return BufferSlice {
                buffer: ManuallyDrop::new(buffer),
                destroyer: self.destroyer.clone(),
                offset: 0,
                length: info.size
            };
        }

        let mut info = info.clone();
        let mut alignment: u64 = 256; // TODO

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
        };
        let mut guard = self.buffers.lock().unwrap();
        let matching_buffers = guard.entry(key).or_insert(Vec::new());

        let mut slice_opt: Option<BufferSlice<B>> = None;
        let mut emptied_buffer_index: Option<usize> = None;
        for (index, sliced_buffer) in matching_buffers.iter_mut().enumerate() {
            if sliced_buffer.slice_size < info.size || sliced_buffer.slice_size % alignment != 0 {
                continue;
            }

            let last_slice = sliced_buffer.free_slices.len() == 1;
            slice_opt = sliced_buffer.free_slices.pop();
            if slice_opt.is_some() {
                if last_slice && index != matching_buffers.len() - 1 {
                    emptied_buffer_index = Some(index);
                }
                break;
            }
        }
        if let Some(index) = emptied_buffer_index {
            // Move now empty buffer to the end of the vector, so we don't have to iterate over it in the future
            let buffer = matching_buffers.remove(index);
            matching_buffers.push(buffer);
        }
        if let Some(slice) = slice_opt {
            return slice;
        }

        let mut slice_opt: Option<BufferSlice<B>> = None;
        let mut refilled_buffer_slice: Option<usize> = None;
        if !matching_buffers.is_empty() {
            // TODO: ref count individual slices to minimize fragmentation
            // the hot path is in the transient buffer allocator anyway

            for (index, sliced_buffer) in matching_buffers.iter_mut().enumerate() {
                if Arc::strong_count(&sliced_buffer.buffer) != 1 {
                    continue;
                }

                // There's only one reference to the buffer, so all slices are unused
                sliced_buffer.reset();
                slice_opt = sliced_buffer.free_slices.pop();

                if slice_opt.is_some() {
                    if index != 0 {
                        refilled_buffer_slice = Some(index);
                    }
                }
            }
        }
        if let Some(index) = refilled_buffer_slice {
            // Move now refilled buffer to the front of the vector, so we find it quickly in the future
            let buffer = matching_buffers.remove(index);
            matching_buffers.insert(0, buffer);
        }
        if let Some(slice) = slice_opt {
            return slice;
        }

        let mut slice_size = align_up_64(info.size, alignment);
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

        let buffer = unsafe {
            Arc::new(self.device.create_buffer(&info, memory_usage, None))
        };

        let mut sliced_buffer = SlicedBuffer::<B> {
            slice_size: slice_size,
            buffer: ManuallyDrop::new(buffer),
            free_slices: Vec::with_capacity(slices as usize),
            destroyer: self.destroyer.clone()
        };
        sliced_buffer.reset();
        let slice = sliced_buffer.free_slices.pop().unwrap();
        matching_buffers.push(sliced_buffer);
        slice
    }
}


pub(crate) fn align_up(value: usize, alignment: usize) -> usize {
  if alignment == 0 {
      return value;
  }
  if value == 0 {
      return 0;
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down(value: usize, alignment: usize) -> usize {
  if alignment == 0 {
      return value;
  }
  (value / alignment) * alignment
}

pub(crate) fn align_up_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
      return value;
  }
  if value == 0 {
      return 0;
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_32(value: u32, alignment: u32) -> u32 {
  if alignment == 0 {
      return value;
  }
  (value / alignment) * alignment
}

pub(crate) fn align_up_64(value: u64, alignment: u64) -> u64 {
  if alignment == 0 {
      return value;
  }
  (value + alignment - 1) & !(alignment - 1)
}

pub(crate) fn align_down_64(value: u64, alignment: u64) -> u64 {
  if alignment == 0 {
      return value;
  }
  (value / alignment) * alignment
}