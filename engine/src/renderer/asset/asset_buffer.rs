use std::sync::{atomic::AtomicU64, Arc};

use smallvec::SmallVec;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use crate::graphics::*;

const DEBUG: bool = false;

/// We suballocate all mesh buffers from a large buffer
/// to be able use indirect rendering.
pub struct AssetBuffer<B: GPUBackend> {
    internal: Arc<AssetBufferInternal<B>>,
}

struct AssetBufferInternal<B: GPUBackend> {
    buffer: Arc<BufferSlice<B>>,
    free_ranges: AtomicRefCell<Vec<BufferRange>>,
    reuse_ranges: AtomicRefCell<Vec<(BufferRange, u32)>>,

    debug_offset: AtomicU64,
    debug_size: u32
}

pub struct AssetBufferSlice<B: GPUBackend> {
    buffer: Arc<AssetBufferInternal<B>>,
    range: BufferRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferRange {
    offset: u32,
    aligned_offset: u32,
    length: u32,
}

impl<B: GPUBackend> AssetBuffer<B> {
    pub const SIZE_BIG: u32 = 256 << 20;
    pub const SIZE_SMALL: u32 = 64 << 20;

    pub fn new(device: &Arc<Device<B>>, size: u32, usage: BufferUsage) -> Self {
        let buffer = device.create_buffer(
            &BufferInfo {
                size: size as u64,
                usage: usage,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            Some("AssetBuffer"),
        ).expect("Failed to allocate geometry buffer.");
        let free_range = BufferRange {
            offset: 0,
            aligned_offset: 0,
            length: size,
        };

        Self {
            internal: Arc::new(AssetBufferInternal {
                buffer,
                free_ranges: AtomicRefCell::new(vec![free_range]),
                reuse_ranges: AtomicRefCell::new(Vec::new()),
                debug_offset: AtomicU64::new(0u64),
                debug_size: size as u32
            }),
        }
    }

    pub fn get_slice(&self, length: usize, alignment: usize) -> AssetBufferSlice<B> {
        if DEBUG {
            let offset = self.internal.debug_offset.fetch_add(length as u64 + alignment as u64, std::sync::atomic::Ordering::SeqCst);
            let aligned_offset = align_up_64(offset, alignment as u64);
            if aligned_offset + length as u64 > self.internal.debug_size as u64 {
                panic!("Ran out of space.");
            }
            return AssetBufferSlice::<B> {
                buffer: self.internal.clone(),
                range: BufferRange {
                    offset: offset as u32,
                    aligned_offset: aligned_offset as u32,
                    length: length as u32,
                },
            };
        }

        let alignment = alignment as u32;

        let mut free_ranges = self.internal.free_ranges.borrow_mut();
        let mut remove_range: bool = false;
        let mut used_range = Option::<(usize, u32)>::None;
        for (index, range) in free_ranges.iter_mut().enumerate() {
            let mut aligned_range = range.clone();
            aligned_range.offset = align_up_32(range.offset, alignment);
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
                aligned_offset: align_up_32(offset, alignment),
                length: length as u32,
            },
        }
    }

    pub fn bump_frame(&self, context: &GraphicsContext<B>) {
        let mut reuse_ranges = self.internal.reuse_ranges.borrow_mut();
        for (range, frames) in reuse_ranges.iter_mut() {
            *frames += 1;
            if *frames > context.prerendered_frames() + 1 {
                self.internal.reuse_range(&range);
            }
        }
        reuse_ranges.retain(|(_r, frames)| *frames <= context.prerendered_frames() + 1);
    }

    pub fn buffer(&self) -> &Arc<BufferSlice<B>> {
        &self.internal.buffer
    }
}

impl<B: GPUBackend> AssetBufferInternal<B> {
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

impl<B: GPUBackend> AssetBufferSlice<B> {
    pub fn buffer(&self) -> &Arc<BufferSlice<B>> {
        &self.buffer.buffer
    }

    pub fn offset(&self) -> u32 {
        self.range.aligned_offset
    }

    pub fn size(&self) -> u32 {
        self.range.length
    }
}

impl<B: GPUBackend> Drop for AssetBufferSlice<B> {
    fn drop(&mut self) {
        if DEBUG {
            return;
        }
        self.buffer.queue_for_reuse(&self.range);
    }
}
