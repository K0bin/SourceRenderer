use std::{sync::{Arc, Mutex}, collections::HashMap, fmt::{Debug, Formatter}, hash::{Hash, Hasher}, mem::ManuallyDrop, ops::Deref};

use sourcerenderer_core::gpu::*;

use super::DeferredDestroyer;

// Using a pointer for the buffer here is technically not safe because it means you could keep the
// slice around after destroying the context and **then** use it with either a new context or one of
// the device methods.
// Should be fine in practice.

enum GPUBufferSliceRef<B: GPUBackend> {
    SharedRef {
        buffer: ManuallyDrop<Arc<B::Buffer>>,
        destroyer: Arc<DeferredDestroyer<B>>
    },
    Pointer(*const B::Buffer)
}

impl<B: GPUBackend> Deref for GPUBufferSliceRef<B> {
    type Target = B::Buffer;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::SharedRef {
                buffer,
                destroyer: _
            } => (*buffer).as_ref(),
            Self::Pointer(ptr) => unsafe { &**ptr }
        }
    }
}

impl<B: GPUBackend> Drop for GPUBufferSliceRef<B> {
    fn drop(&mut self) {
        match self {
            Self::SharedRef {
                ref mut buffer,
                ref destroyer
            } => {
                let buffer = unsafe { ManuallyDrop::take(&mut *buffer) };
                destroyer.destroy_buffer_reference(buffer);
            },
            _ => {}
        }
    }
}

impl<B: GPUBackend> PartialEq for GPUBufferSliceRef<B> {
    fn eq(&self, other: &Self) -> bool {
        let self_ref: &B::Buffer = self;
        let other_ref: &B::Buffer = other;
        self_ref == other_ref
    }
}

impl<B: GPUBackend> Clone for GPUBufferSliceRef<B> {
    fn clone(&self) -> Self {
        match self {
            Self::SharedRef {
                buffer,
                destroyer
            } => Self::SharedRef { buffer: buffer.clone(), destroyer: destroyer.clone() },
            Self::Pointer(ptr) => Self::Pointer(*ptr)
        }
    }
}

unsafe impl<B: GPUBackend> Send for GPUBufferSlice<B> {}
unsafe impl<B: GPUBackend> Sync for GPUBufferSlice<B> {}

pub struct GPUBufferSlice<B: GPUBackend> {
  buffer: GPUBufferSliceRef<B>,
  offset: u64,
  length: u64
}

impl<B: GPUBackend> Debug for GPUBufferSlice<B> {
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

impl<B: GPUBackend> Hash for GPUBufferSlice<B> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
        self.offset.hash(state);
        self.length.hash(state);
    }
}

impl<B: GPUBackend> PartialEq for GPUBufferSlice<B> {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
            && self.length == other.length
            && self.offset == other.offset
    }
}

impl<B: GPUBackend> Eq for GPUBufferSlice<B> {}

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

enum GPUBufferSliceBuffer<B: GPUBackend> {
    SharedRef(Arc<B::Buffer>),
    Owned(B::Buffer)
}

impl<B: GPUBackend> Deref for GPUBufferSliceBuffer<B> {
    type Target = B::Buffer;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::SharedRef(buffer) => buffer.as_ref(),
            Self::Owned(buffer) => buffer,
        }
    }
}

struct SlicedBuffer<B: GPUBackend> {
    slice_size: u64,
    buffer: ManuallyDrop<GPUBufferSliceBuffer<B>>,
    free_slices: Vec<GPUBufferSlice<B>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for SlicedBuffer<B> {
    fn drop(&mut self) {
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        match buffer {
            GPUBufferSliceBuffer::Owned(buffer) => { self.destroyer.destroy_buffer(buffer); }
            GPUBufferSliceBuffer::SharedRef(buffer_ref) => { self.destroyer.destroy_buffer_reference(buffer_ref); }
        }
    }
}

impl<B: GPUBackend> SlicedBuffer<B> {
    pub(crate) fn reset(&mut self) {
        let slices = (SLICED_BUFFER_SIZE / self.slice_size).max(1);
        for i in 0..slices {
          let slice: GPUBufferSlice<_> = GPUBufferSlice {
                buffer: match &*self.buffer {
                    GPUBufferSliceBuffer::SharedRef(buffer_arc) => GPUBufferSliceRef::SharedRef {
                        buffer: ManuallyDrop::new(buffer_arc.clone()),
                        destroyer: self.destroyer.clone()
                    },
                    GPUBufferSliceBuffer::Owned(buffer) => GPUBufferSliceRef::Pointer(buffer as *const B::Buffer),
                },
                offset: i * self.slice_size,
                length: self.slice_size
          };
          self.free_slices.push(slice);
        }
    }
}

pub struct GPUBufferAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    buffers: Mutex<HashMap<BufferKey, Vec<SlicedBuffer<B>>>>,
    transient: bool,
}

impl<B: GPUBackend> GPUBufferAllocator<B> {

    pub fn get_slice(
      &self,
      info: &BufferInfo,
      memory_usage: MemoryUsage,
      name: Option<&str>,
    ) -> GPUBufferSlice<B> {

        if info.size > BIG_BUFFER_SLAB_SIZE && !self.transient {
            // Don't do one-off buffers for command lists
            let buffer = Arc::new(
                unsafe {
                self.device.create_buffer(info, memory_usage, name)
                }
            );
            return GPUBufferSlice {
                buffer: GPUBufferSliceRef::SharedRef {
                    buffer: ManuallyDrop::new(buffer),
                    destroyer: self.destroyer.clone()
                },
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
        // TODO: consider a smarter data structure than a simple list of all slices regardless of size.

        let mut slice_opt: Option<GPUBufferSlice<B>> = None;
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

        if !self.transient && !matching_buffers.is_empty() {
            for sliced_buffer in matching_buffers.iter_mut() {
                let buffer = if let GPUBufferSliceBuffer::SharedRef(buffer) = &*sliced_buffer.buffer {
                    buffer
                } else {
                    unreachable!();
                };

                if Arc::strong_count(buffer) != 1 {
                    continue;
                }

                // There's only one reference to the buffer, so all slices are unused
                sliced_buffer.reset();

                return sliced_buffer.free_slices.pop().unwrap();
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
            self.device.create_buffer(&info, memory_usage, None)
        };

        let mut sliced_buffer = SlicedBuffer::<B> {
            slice_size: slice_size,
            buffer: ManuallyDrop::new(if self.transient {
                GPUBufferSliceBuffer::Owned(buffer)
            } else {
                GPUBufferSliceBuffer::SharedRef(Arc::new(buffer))
            }),
            free_slices: Vec::with_capacity(slices as usize),
            destroyer: self.destroyer.clone()
        };
        sliced_buffer.reset();
        let slice = sliced_buffer.free_slices.pop().unwrap();
        matching_buffers.push(sliced_buffer);
        slice
  }

  pub fn reset(&self) {
    if !self.transient {
        return;
    }

    let mut buffers_types = self.buffers.lock().unwrap();
    for (_key, buffers) in buffers_types.iter_mut() {
        for sliced_buffer in buffers.iter_mut() {
            sliced_buffer.reset();
        }
        buffers.sort_unstable_by_key(|a| a.slice_size);
    }
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