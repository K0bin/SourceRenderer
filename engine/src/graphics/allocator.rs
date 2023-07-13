use std::{sync::Arc, sync::Mutex};

use smallvec::SmallVec;

pub(super) struct Chunk<T>
    where T : Send + Sync
{
    inner: Arc<Mutex<ChunkInner>>,
    data: Arc<T>
}

struct ChunkInner
{
    free_list: SmallVec<[Range; 16]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Range {
    offset: u64,
    length: u64
}

pub(super) struct Allocation<T>
    where T : Send + Sync
{
    inner: Arc<Mutex<ChunkInner>>,
    data: Arc<T>,
    pub offset: u64,
    pub length: u64
}

impl<T> Allocation<T>
    where T : Send + Sync
{
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    #[inline(always)]
    pub fn length(&self) -> u64 {
        self.length
    }

    #[inline(always)]
    pub fn data(&self) -> &T {
        &*self.data
    }

    #[inline(always)]
    pub fn data_arc(&self) -> &Arc<T> {
        &self.data
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
            inner: Arc::new(Mutex::new(ChunkInner {
                free_list
            })),
            data: Arc::new(data),
        }
    }

    pub fn allocate(&self, size: u64, alignment: u64) -> Option<Allocation<T>> {
        let mut inner = self.inner.lock().unwrap();

        let mut best = Option::<(usize, Range)>::None;
        for (index, range) in inner.free_list.iter().enumerate() {
            if (range.offset % alignment) != 0 || range.length < size {
                continue;
            }

            if range.length == size {
                best = Some((index, range.clone()));
                break;
            }

            if let Some((best_index, best_range)) = best.clone() {
                if range.length < best_range.length {
                    best = Some((index, range.clone()));
                }
            } else {
                best = Some((index, range.clone()));
            }
        }

        best.map(|(free_index, range)| {
            if range.length == size {
                inner.free_list.remove(free_index);
            } else {
                let existing_range = &mut inner.free_list[free_index];
                existing_range.offset += size;
                existing_range.length -= size;
            }
            Allocation {
                inner: self.inner.clone(),
                data: self.data.clone(),
                offset: range.offset,
                length: size
            }
        })
    }
}

impl<T> Drop for Allocation<T>
    where T : Send + Sync
{
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();

        let mut i = 0usize;
        while i < inner.free_list.len() {
            let drop_i = {
                let range = &inner.free_list[i];

                if range.offset == self.offset + self.length {
                    self.length += range.length;
                    true
                } else if range.offset + range.length == self.offset {
                    self.offset = range.offset;
                    self.length += range.length;
                    true
                } else {
                    false
                }
            };
            if drop_i {
                inner.free_list.remove(i);
            } else {
                i += 1;
            }
        }

        inner.free_list.push(Range {
            offset: self.offset,
            length: self.length
        });
    }
}