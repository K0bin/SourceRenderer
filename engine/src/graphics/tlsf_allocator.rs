use std::{alloc::Layout, sync::{Arc, Mutex}};
use std::num::NonZeroUsize;

use smallvec::SmallVec;

/*
 * The maximum number of L1 items: just use 2^64 on 64-bit architectures.
 * Otherwise, log2(4 GB) = 32.
 */
const TLSF_FLI_MAX: usize = 8usize * std::mem::size_of::<u64>();

/*
 * The number of subdivisions (second-level-index), expressed as an
 * exponent of 2 for bitwise shifting.  2^5 = 32 subdivisions.
 */
const TLSF_SLI_SHIFT: usize = 5usize;
const TLSF_SLI_MAX: usize = 1usize << TLSF_SLI_SHIFT;

/*
 * Default minimum block size.
 */
const TLSF_MBS_DEFAULT: usize = 32usize;

/*
 * Each memory block is tracked using a block header.  There are two
 * cases: TLSF-INT and TLSF-EXT i.e. internalised or externalised block
 * header use.
 *
 * - Free blocks are additionally linked within their size class.
 *
 * - The length field stores the block length excluding the header.
 */
const TLSF_BLK_FREE: usize = !(std::mem::size_of::<usize>() >> 1usize);

// http://www.gii.upv.es/tlsf/files/papers/ecrts04_tlsf.pdf

struct TlsfBlock<T>
    where T : Send + Sync {
    range: Range,
    is_free: bool,

    next_idx: BlockIdxOpt,
    prev_idx: BlockIdxOpt
}

struct TlsfTopLevelArrayEntry {
    free_indices: [ BlockIdxOpt; TLSF_SLI_MAX ],
    bitmap: u64
}

impl Default for TlsfTopLevelArrayEntry {
    fn default() -> Self {
        Self {
            free_indices: [ Option::None; TLSF_SLI_MAX ],
            bitmap: 0u64
        }
    }
}

type BlockIdxOpt = Option<BlockIdx>;
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct BlockIdx(NonZeroUsize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct FirstLevelIdx(usize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct SecondLevelIdx(usize);

struct RemovedBlock<'a, T : Send + Sync> {
    block: &'a mut TlsfBlock<T>,
    block_idx: BlockIdx,
    next_block_idx: BlockIdxOpt
}

struct TlsfChunk<T>
    where T : Send + Sync {
    inner: Arc<Mutex<TslfChunkInner<T>>>
}

struct TslfChunkInner<T>
    where T : Send +  Sync {
    data: Arc<T>,
    size: u64,
    free: u64,

    map: [ TlsfTopLevelArrayEntry; TLSF_FLI_MAX ],
    top_level_bitmap: u64,
    blocks: Vec<TlsfBlock<T>>
}

fn get_mapping(size: u64) -> (FirstLevelIdx, SecondLevelIdx) {
    let f = size.leading_zeros();
    let mask = (1 << (TLSF_SLI_SHIFT + 1)) - 1;
    let s = ((size << f) >> f) & mask;
    (FirstLevelIdx(f as usize), SecondLevelIdx(s as usize))
}

pub(crate) fn align_up_64(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

fn get_block<'a, T: Send + Sync>(blocks: &'a [TlsfBlock<T>], idx: BlockIdx) -> &'a TlsfBlock<T> {
    &blocks[idx.0.get() - 1]
}

fn get_block_mut<'a, T: Send + Sync>(blocks: &'a mut [TlsfBlock<T>], idx: BlockIdx) -> &'a mut TlsfBlock<T> {
    &mut blocks[idx.0.get() - 1]
}

fn get_block_opt<'a, T: Send + Sync>(blocks: &'a [TlsfBlock<T>], idx: BlockIdxOpt) -> Option<&'a TlsfBlock<T>> {
    idx.map(|idx| &blocks[idx.0.get() - 1])
}

fn get_block_opt_mut<'a, T: Send + Sync>(blocks: &'a mut [TlsfBlock<T>], idx: BlockIdxOpt) -> Option<&'a mut TlsfBlock<T>> {
    idx.map(|idx| &mut blocks[idx.0.get() - 1])
}

impl<T> TslfChunkInner<T>
    where T : Send + Sync {

    fn get_block_idx(&self, fl: FirstLevelIdx, sl: SecondLevelIdx) -> BlockIdxOpt {
        (&self.map[fl.0])[sl.1]
    }

    fn get_block_idx_mut(&mut self, fl: FirstLevelIdx, sl: SecondLevelIdx) -> &mut BlockIdxOpt {
        &mut (&mut self.map[fl.0])[sl.1]
    }

    fn remove_block(&mut self, fl: FirstLevelIdx, sl: SecondLevelIdx) -> Option<RemovedBlock<T>> {
        let idx: BlockIdx;
        let next_idx: BlockIdxOpt;
        {
            let idx_opt_mut: &mut BlockIdxOpt = self.get_block_idx_mut(fl, sl);
            if idx_opt_mut.is_none() {
                return None;
            }
            idx = idx_opt_mut.unwrap();
            let block = get_block_mut(&mut self.blocks, idx);
            next_idx = block.next_idx;
            *idx_opt_mut = std::mem::replace(&mut block.next_idx, None);
            debug_assert_eq!(block.prev_idx, None);
        }
        if let Some(next_idx) = next_idx {
            let block = get_block_mut(&mut self.blocks, next_idx);
            let prev_idx_before_change = std::mem::replace(&mut block.prev_idx, None);
            debug_assert_eq!(prev_idx_before_change, Some(idx));
        }
        Some(RemovedBlock {
            block: get_block_mut(&mut self.blocks, idx),
            block_idx: idx,
            next_block_idx: next_idx
        })
    }

    fn insert_block(&mut self, idx: BlockIdx) {
        let block = get_block_mut(&mut self.blocks, idx);
        let size = block.range.length;
        let (f, s) = get_mapping(size);
        let head_idx = self.get_block_idx_mut(f, s);
        let old_head_idx = head_idx;
        *head_idx = Some(idx);
        block.next_idx = old_head_idx;
        std::mem::forget(block);
        if let Some(idx) = old_head_idx {
            let block = get_block_mut(&mut self.blocks, idx);
            let prev_idx_before_change = std::mem::replace(&mut block.prev_idx, Some(idx));
            assert_eq!(prev_idx_before_change, None);
        }
    }
}

impl<T> TlsfChunk<T>
    where T : Send + Sync {
    pub fn new(data: T, chunk_size: u64) -> Self {
        let data_arc = Arc::new(data);
        let blocks = vec![
            TlsfBlock {
                data: data_arc.clone(),
                range: Range {
                    offset: 0u64,
                    length: chunk_size,
                },
                is_free: true,
                next_idx: None,
                prev_idx: None
            }
        ];

        let mut map: [ TlsfTopLevelArrayEntry; TLSF_SLI_MAX as usize ] = Default::default();
        map[0].free_indices[0] = Some(NonZeroUsize::new(1).unwrap());
        map[0].bitmap = 1;

        TlsfChunk::<T> {
            data: data_arc,
            size: chunk_size,
            free: chunk_size,
            first_idx: Some(NonZeroUsize::new(1).unwrap()),
            last_idx: Some(NonZeroUsize::new(1).unwrap()),
            blocks: Mutex::new(blocks),
            map,
            top_level_bitmap: 1
        }
    }

    pub fn allocate(&self, size: u64, mut alignment: u64) -> Option<Allocation<T>> {
        let mut inner = self.inner.lock().unwrap();
        alignment = alignment.max(4);
        let (mut f, mut s) = get_mapping(size);

        let mut blocks = self.blocks.lock().unwrap();
        let map_entry = &mut (&self.map[f]).free_indices[s];

        if let Some(RemovedBlock {
            block,
            block_idx,
            next_block_idx
        }) = inner.remove_block(&mut blocks, map_entry) {
            std::mem::drop(map_entry);
            if next_block_idx.is_none() {
                inner.map[f].bitmap &= !(1 << s);
                if inner.map[f].bitmap == 0 {
                    inner.top_level_bitmap &= !(1 << f);
                }
            }
            return Some(Allocation {
                inner: self.inner.clone(),
                data_ptr: inner.data.as_ref() as *const T,
                range: block.range.clone()
            });
        } else {
            if s < TLSF_SLI_MAX - 1 && self.map[f].bitmap != 0 {
                s += ((self.map[f].bitmap >> s) << s).trailing_zeros() as usize;
            } else if f < TLSF_FLI_MAX - 1 && self.top_level_bitmap != 0 {
                f += ((self.top_level_bitmap >> f) << f).trailing_zeros() as usize;
            } else {
                return None;
            }
            let map_entry: &mut Option<NonZeroUsize> = &mut (&self.map[f]).free_indices[s];
            let bigger_block_opt = inner.remove_block(&mut blocks, map_entry);
            if bigger_block_opt.is_none() {
                return None;
            }
            let RemovedBlock { block, block_idx, next_block_idx } = bigger_block_opt.unwrap();
            let remaining_max_size = block.range.length - size;
        }


        let block_opt = get_block_mut(&mut blocks, *map_entry);
        if let Some(block) = block_opt {
            let range = block.range.clone();
            let next_idx = block.next_idx;
            *map_entry = block.next_idx;
            block.next_idx = None;
            assert_eq!(block.prev_idx, None);
            std::mem::drop(block);
            let next_block = get_block_mut(&mut blocks, next_idx);
            if let Some(next_block) = next_block {
                next_block.prev_idx = None;
            }
            std::mem::drop(next_block);
            return Some(Allocation {
                data: self.data.clone(),
                data_ptr: &self.inner.data as *const T,
                range: block.range.clone()
            });
        } else {
            if s < TLSF_SLI_MAX - 1 {
                s += 1;
            } else if f < TLSF_FLI_MAX - 1 {
                f += 1;
            } else {
                return None;
            }

            let map_entry: &mut Option<NonZeroUsize> = &mut (&self.map[f]).free_indices[s];
            let block_opt = get_block(&blocks, map_entry)
                .expect("Block was expected to not be empty. Bitmap is incorrect.");

            *map_entry = None;



        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub offset: u64,
    pub length: u64
}

pub(super) struct Allocation<T>
    where T : Send + Sync
{
    inner: Arc<TlsfChunkInner<T>>,
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

