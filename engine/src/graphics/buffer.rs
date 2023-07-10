use std::{sync::{Arc, Mutex}, collections::HashMap, fmt::{Debug, Formatter}, hash::Hash, mem::ManuallyDrop};

use sourcerenderer_core::gpu::*;

use super::*;

pub struct BufferSlice<B: GPUBackend> {
  pub(crate) buffer: Arc<B::Buffer>,
  pub offset: u64,
  pub length: u64
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

struct BufferSliceCollection<B: GPUBackend> {
    free_slices: Vec<Arc<BufferSlice<B>>>,
    used_slices: Vec<Arc<BufferSlice<B>>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for BufferSliceCollection<B> {
    fn drop(&mut self) {
        for slice in self.free_slices.drain(..) {
            self.destroyer.destroy_buffer_slice_reference(slice);
        }
        for slice in self.used_slices.drain(..) {
            self.destroyer.destroy_buffer_slice_reference(slice);
        }
    }
}

pub struct BufferAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    buffers: Mutex<HashMap<BufferKey, BufferSliceCollection<B>>>,
}

impl<B: GPUBackend> BufferAllocator<B> {

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      name: Option<&str>,
    ) -> Arc<BufferSlice<B>> {

        if info.size > BIG_BUFFER_SLAB_SIZE {
            // Don't do one-off buffers for command lists
            let buffer = unsafe {
                Arc::new(self.device.create_buffer(info, memory_usage, name))
            };
            return Arc::new(BufferSlice {
                buffer: buffer,
                offset: 0,
                length: info.size
            });
        }

        let mut info = info.clone();
        let mut alignment: u64 = 256; // TODO

        let key = BufferKey {
            memory_usage,
            buffer_usage: info.usage,
        };
        let mut guard = self.buffers.lock().unwrap();
        let matching_buffers = guard.entry(key).or_insert_with(|| BufferSliceCollection {
            free_slices: Vec::new(),
            used_slices: Vec::new(),
            destroyer: self.destroyer.clone()
        });

        let mut found_slice_index: Option<usize> = None;
        for (index, slice) in matching_buffers.free_slices.iter().enumerate() {
            if slice.length >= info.size && slice.offset % alignment == 0 {
                found_slice_index = Some(index);
                break;
            }
        }
        if let Some(index) = found_slice_index {
            return matching_buffers.free_slices.remove(index);
        }

        let mut slice_opt: Option<BufferSlice<B>> = None;
        let mut refilled_buffer_slice: Option<usize> = None;
        if !matching_buffers.used_slices.is_empty() {
            // This is awful. Completely rewrite this with drain_filter once that's stabilized.
            // Right now cleaner alternatives would likely need to do more copying and allocations.
            let length = matching_buffers.used_slices.len();
            for i in (0..length).rev() {
                let refcount = {
                    let slice = &matching_buffers.used_slices[i];
                    Arc::strong_count(slice)
                };
                if refcount == 1 {
                    matching_buffers
                        .free_slices
                        .push(matching_buffers.used_slices.remove(i));
                }
            }
            matching_buffers.free_slices.sort_by_key(|b| b.length);
            
            found_slice_index = None;
            for (index, slice) in matching_buffers.free_slices.iter().enumerate() {
                if slice.length >= info.size && slice.offset % alignment == 0 {
                    found_slice_index = Some(index);
                    break;
                }
            }
            if let Some(index) = found_slice_index {
                return matching_buffers.free_slices.remove(index);
            }
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

        for i in 0..(slices - 1) {
            let slice = Arc::new(BufferSlice::<B> {
                buffer: buffer.clone(),
                offset: i * slice_size,
                length: slice_size
            });
            matching_buffers.free_slices.push(slice);
        }

        let slice = Arc::new(BufferSlice::<B> {
            buffer,
            offset: (slices - 1) * slice_size,
            length: slice_size
        });
        matching_buffers.used_slices.push(slice.clone());
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