use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::{graphics::{BufferInfo, Backend, BufferUsage, Device, MemoryUsage}, atomic_refcell::AtomicRefCell};

/// We suballocate all mesh buffers from a large buffer
/// to be able use indirect rendering.
pub struct AssetBuffer<B: Backend> {
  internal: Arc<AssetBufferInternal<B>>
}

struct AssetBufferInternal<B: Backend> {
  buffer: Arc<B::Buffer>,
  free_ranges: AtomicRefCell<Vec<BufferRange>>,
  reuse_ranges: AtomicRefCell<Vec<(BufferRange, u32)>>,
}

pub struct AssetBufferSlice<B: Backend> {
  buffer: Arc<AssetBufferInternal<B>>,
  range: BufferRange
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferRange {
  offset: u32,
  aligned_offset: u32,
  length: u32
}

impl<B: Backend> AssetBuffer<B> {
  pub const SIZE_BIG: u32 = 128 << 20;
  pub const SIZE_SMALL: u32 = 32 << 20;
  pub fn new(device: &Arc<B::Device>, size: u32, usage: BufferUsage) -> Self {
    let buffer = device.create_buffer(&BufferInfo {
      size: size as usize,
      usage: usage,
    }, MemoryUsage::GpuOnly, Some("AssetBuffer"));
    let free_range = BufferRange {
      offset: 0,
      aligned_offset: 0,
      length: size
    };

    Self {
      internal: Arc::new(
        AssetBufferInternal {
          buffer,
          free_ranges: AtomicRefCell::new(vec![free_range]),
          reuse_ranges: AtomicRefCell::new(Vec::new()),
        }
      ),
    }
  }

  pub fn get_slice(&self, length: usize, alignment: usize) -> AssetBufferSlice<B> {
    let alignment = alignment as u32;

    let mut free_ranges = self.internal.free_ranges.borrow_mut();
    let mut remove_range: bool = false;
    let mut used_range = Option::<(usize, u32)>::None;
    for (index, range) in free_ranges.iter_mut().enumerate() {
      let mut aligned_range = range.clone();
      aligned_range.offset = ((aligned_range.offset + alignment - 1) / alignment) * alignment;
      let alignment_diff = aligned_range.offset - range.offset;
      aligned_range.length -= alignment_diff;
      if aligned_range.length >= length as u32 {
        used_range = Some((index, range.offset));
        if range.length != length as u32 {
          range.offset += length as u32 + alignment_diff;
          range.length -= length as u32 + alignment_diff;
        } else {
          remove_range = true;
        }
        break;
      }
    }

    let (index, offset) = used_range.expect("Could not find enough space in the AssetBuffer");
    if remove_range {
      free_ranges.remove(index);
    }

    AssetBufferSlice::<B> {
      buffer: self.internal.clone(),
      range: BufferRange {
        offset,
        aligned_offset: ((offset + alignment - 1) / alignment) * alignment,
        length: length as u32
      }
    }
  }

  pub fn bump_frame(&self, device: &Arc<B::Device>) {
    let mut reuse_ranges = self.internal.reuse_ranges.borrow_mut();
    for (range, frames) in reuse_ranges.iter_mut() {
      *frames += 1;
      if *frames > device.prerendered_frames() + 1 {
        self.internal.reuse_range(&range);
      }
    }
    reuse_ranges.retain(|(_r, frames)| *frames <= device.prerendered_frames() + 1);
  }

  pub fn buffer(&self) -> &Arc<B::Buffer> {
    &self.internal.buffer
  }
}

impl<B: Backend> AssetBufferInternal<B> {
  pub fn queue_for_reuse(&self, range: &BufferRange) {
    let mut reuse_ranges = self.reuse_ranges.borrow_mut();
    reuse_ranges.push((range.clone(), 0));
  }

  pub fn reuse_range(&self, range: &BufferRange) {
    let mut free_ranges = self.free_ranges.borrow_mut();
    let mut indices_to_delete = SmallVec::<[u32; 16]>::new();
    let mut range = range.clone();

    for (index, entry) in free_ranges.iter().enumerate() {
      if entry.offset == range.offset + range.length {
        range.length += entry.length;
        indices_to_delete.push(index as u32);
      } else if entry.offset + entry.offset == range.offset {
        range.offset -= entry.length;
        indices_to_delete.push(index as u32);
      }
    }

    let mut deleted_count = 0;
    for index_to_delete in indices_to_delete {
      free_ranges.remove(index_to_delete as usize - deleted_count);
      deleted_count += 1;
    }

    free_ranges.push(range);
  }
}

impl<B: Backend> AssetBufferSlice<B> {
  pub fn buffer(&self) -> &Arc<B::Buffer> {
    &self.buffer.buffer
  }

  pub fn offset(&self) -> u32 {
    self.range.offset
  }

  pub fn size(&self) -> u32 {
    self.range.length
  }
}

impl<B: Backend> Drop for AssetBufferSlice<B> {
  fn drop(&mut self) {
    self.buffer.queue_for_reuse(&self.range);
  }
}
