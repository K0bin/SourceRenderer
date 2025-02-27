use std::sync::{atomic::AtomicU64, Arc};
use crate::Mutex;

use smallvec::SmallVec;

use super::align_up_64;

// TODO: Implement Two Level Seggregate Fit allocator

const DEBUG: bool = false;

pub(super) struct Chunk<T>
    where T : Send + Sync
{
    inner: Arc<ChunkInner<T>>,
    size: u64
}

struct ChunkInner<T>
where T : Send + Sync {
    free_list: Mutex<SmallVec<[Range; 16]>>,
    data: T,
    free_callback: Option<Box<dyn Fn(&[Range]) + Send + Sync>>,

    debug_offset: AtomicU64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub offset: u64,
    pub length: u64
}

pub(super) struct Allocation<T>
    where T : Send + Sync
{
    inner: Arc<ChunkInner<T>>,
    data_ptr: *const T,
    pub range: Range,
}

unsafe impl<T> Send for Allocation<T> where T : Send + Sync {}
unsafe impl<T> Sync for Allocation<T> where T : Send + Sync {}

impl<T> Allocation<T>
    where T : Send + Sync
{
    #[allow(unused)]
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.range.offset
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn length(&self) -> u64 {
        self.range.length
    }

    #[inline(always)]
    pub fn data(&self) -> &T {
        unsafe {
            &*self.data_ptr
        }
    }
}

impl<T> Chunk<T>
    where T : Send + Sync
{
    pub fn new(data: T, chunk_size: u64) -> Self {
        let mut free_list = SmallVec::<[Range; 16]>::new();
        free_list.push(Range {
            offset: 0u64,
            length: chunk_size
        });
        Self {
            inner: Arc::new(ChunkInner {
                free_list: Mutex::new(free_list),
                data,
                free_callback: None,
                debug_offset: AtomicU64::new(0u64)
            }),
            size: chunk_size
        }
    }

    #[allow(unused)]
    pub fn with_callback<F>(data: T, chunk_size: u64, free_callback: F) -> Self
    where F: Fn(&[Range]) + Send + Sync + 'static {
        let mut free_list = SmallVec::<[Range; 16]>::new();
        free_list.push(Range {
            offset: 0u64,
            length: chunk_size
        });
        Self {
            inner: Arc::new(ChunkInner {
                free_list: Mutex::new(free_list),
                data,
                free_callback: Some(Box::new(free_callback)),
                debug_offset: AtomicU64::new(0u64)
            }),
            size: chunk_size
        }
    }

    pub fn allocate(&self, size: u64, alignment: u64) -> Option<Allocation<T>> {
        if DEBUG {
            let offset = self.inner.debug_offset.fetch_add(size + alignment, std::sync::atomic::Ordering::SeqCst);
            let aligned_offset = align_up_64(offset, alignment);
            if aligned_offset + size > self.size {
                return None;
            }
            return Some(Allocation {
                inner: self.inner.clone(),
                data_ptr: &self.inner.data as *const T,
                range: Range {
                    offset: aligned_offset,
                    length: size
                }
            });
        }

        let mut free_list = self.inner.free_list.lock().unwrap();

        let mut best = Option::<(usize, Range)>::None;
        for (index, range) in free_list.iter().enumerate() {
            if size == 1 {
                best = Some((index, range.clone()));
                break;
            }

            let aligned_offset = align_up_64(range.offset, alignment);
            let alignment_diff = aligned_offset - range.offset;

            if range.length < size + alignment_diff {
                continue;
            }

            if range.length == size {
                best = Some((index, range.clone()));
                break;
            }

            if let Some((_best_index, best_range)) = best.clone() {
                if range.length < best_range.length {
                    best = Some((index, range.clone()));
                }
            } else {
                best = Some((index, range.clone()));
            }
        }

        best.map(|(mut free_index, mut range)| {
            let aligned_offset = align_up_64(range.offset, alignment);
            let alignment_diff = aligned_offset - range.offset;
            let consume_entire_range = range.length == size + alignment_diff;
            range.length = size;

            if alignment_diff != 0 {
                // Push chosen range back to fit alignment and add a new one before that
                free_list.insert(free_index, Range {
                    offset: range.offset,
                    length: alignment_diff
                });
                range.offset += alignment_diff;
                free_index += 1;
            }

            if consume_entire_range {
                free_list.remove(free_index);
            } else {
                let existing_range = &mut free_list[free_index];
                debug_assert!(existing_range.length > size + alignment_diff);
                existing_range.offset += size + alignment_diff;
                existing_range.length -= size + alignment_diff;
            }

            Allocation {
                inner: self.inner.clone(),
                data_ptr: &self.inner.data as *const T,
                range
            }
        })
    }

    pub fn is_empty(&self) -> bool {
        let free_list = self.inner.free_list.lock().unwrap();
        if free_list.len() != 1 {
            return false;
        }
        let first = free_list.first().unwrap();
        first.offset == 0 && first.length == self.size
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl<T> Drop for Allocation<T>
    where T : Send + Sync
{
    fn drop(&mut self) {
        if DEBUG {
            return;
        }

        let mut free_list = self.inner.free_list.lock().unwrap();

        let mut insert_position = Option::<usize>::None;
        let mut i = 0usize;
        while i < free_list.len() {
            let drop_i = {
                let range = &free_list[i];

                if range.offset > self.range.offset + self.range.length {
                    insert_position = Some(i);
                    break;
                }

                if range.offset == self.range.offset + self.range.length {
                    self.range.length += range.length;
                    true
                } else if range.offset + range.length == self.range.offset {
                    self.range.offset = range.offset;
                    self.range.length += range.length;
                    true
                } else {
                    false
                }
            };
            if drop_i {
                free_list.remove(i);
            } else {
                i += 1;
            }
        }

        if let Some(insert_position) = insert_position {
            free_list.insert(insert_position, self.range.clone());
        } else {
            free_list.push(self.range.clone());
        }
        if let Some(callback) = self.inner.free_callback.as_ref() {
            callback(&free_list[..]);
        }
    }
}
