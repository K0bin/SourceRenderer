use std::{sync::Arc, sync::Mutex};

use smallvec::SmallVec;

// TODO: Implement Two Level Seggregate Fit allocator

pub(super) struct Chunk<T>
    where T : Send + Sync
{
    inner: Arc<ChunkInner<T>>,
}

struct ChunkInner<T>
where T : Send + Sync {
    free_list: Mutex<SmallVec<[Range; 16]>>,
    data: T
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
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.range.offset
    }

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
                data
            }),
        }
    }

    pub fn allocate(&self, size: u64, alignment: u64) -> Option<Allocation<T>> {
        let mut free_list = self.inner.free_list.lock().unwrap();

        let mut best = Option::<(usize, Range)>::None;
        for (index, range) in free_list.iter().enumerate() {
            if (range.offset % alignment) != 0 || range.length < size {
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

        best.map(|(free_index, range)| {
            if range.length == size {
                free_list.remove(free_index);
            } else {
                let existing_range = &mut free_list[free_index];
                existing_range.offset += size;
                existing_range.length -= size;
            }
            Allocation {
                inner: self.inner.clone(),
                data_ptr: &self.inner.data as *const T,
                range: Range {
                    offset: range.offset,
                    length: size
                }
            }
        })
    }
}

impl<T> Drop for Allocation<T>
    where T : Send + Sync
{
    fn drop(&mut self) {
        let mut free_list = self.inner.free_list.lock().unwrap();

        let mut i = 0usize;
        while i < free_list.len() {
            let drop_i = {
                let range = &free_list[i];

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

        free_list.push(self.range.clone());
    }
}
