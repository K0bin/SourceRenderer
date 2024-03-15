use std::{sync::{Mutex, Arc}, collections::HashMap};

use log::trace;
use sourcerenderer_core::gpu::*;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryUsage {
    GPUMemory,
    MainMemoryCached,
    MainMemoryWriteCombined,
    MappableGPUMemory
}

pub type MemoryTypeIndex = u32;
pub type MemoryTypeMask = u32;

pub(super) struct MemoryAllocator<B: GPUBackend> {
    device: Arc<B::Device>,
    is_uma: bool,
    inner: Mutex<MemoryAllocatorInner<B>>
}

pub(super) struct MemoryAllocatorInner<B: GPUBackend> {
    chunks: HashMap<MemoryTypeIndex, Vec<Chunk<B::Heap>>>
}

const CHUNK_SIZE: u64 = 256 << 20;

pub(super) type MemoryAllocation<H> = Allocation<H>;

#[derive(Debug)]
pub(super) enum MemoryTypeMatchingStrictness {
    ForceCoherent,
    Normal,
    Fallback
}

impl<B: GPUBackend> MemoryAllocator<B> {
    pub(super) fn new(device: &Arc<B::Device>) -> Self {
        let memory_types = unsafe { device.memory_type_infos() };
        let is_uma = memory_types.iter().all(|memory_type| memory_type.is_cpu_accessible && memory_type.memory_kind == MemoryKind::VRAM);

        Self {
            device: device.clone(),
            is_uma,
            inner: Mutex::new(MemoryAllocatorInner {
                chunks: HashMap::new()
            })
        }
    }

    fn allocate_by_memory_type(&self, memory_type_index: MemoryTypeIndex, size: u64, alignment: u64) -> Result<MemoryAllocation<B::Heap>, OutOfMemoryError> {
        let mut inner = self.inner.lock().unwrap();
        let chunk_list = inner.chunks.entry(memory_type_index).or_insert(Vec::new());
        let allocation = chunk_list.iter().find_map(|chunk| chunk.allocate(size, alignment));
        if let Some(allocation) = allocation {
            return Ok(allocation);
        }

        let heap = unsafe { self.device.create_heap(memory_type_index, CHUNK_SIZE) };
        if heap.is_err() {
            return Err(OutOfMemoryError {  });
        }
        let heap = heap.unwrap();
        let chunk = Chunk::new(heap, CHUNK_SIZE.max(size));
        let allocation = chunk.allocate(size, alignment).unwrap();
        chunk_list.push(chunk);
        Ok(allocation)
    }

    pub(super) fn allocate(&self, usage: MemoryUsage, requirements: &ResourceHeapInfo) -> Result<MemoryAllocation<B::Heap>, OutOfMemoryError> {
        let mut mask: u32;

        if usage != MemoryUsage::GPUMemory {
            mask = self.find_memory_type_mask(usage,  MemoryTypeMatchingStrictness::ForceCoherent) & requirements.memory_type_mask;
            if let Ok(allocation) = self.try_allocate(mask, requirements.size, requirements.alignment) {
                return Ok(allocation);
            }
        }

        mask = self.find_memory_type_mask(usage,  MemoryTypeMatchingStrictness::Normal) & requirements.memory_type_mask;
        if let Ok(allocation) = self.try_allocate(mask, requirements.size, requirements.alignment) {
            return Ok(allocation);
        }

        mask = self.find_memory_type_mask(usage,  MemoryTypeMatchingStrictness::Fallback) & requirements.memory_type_mask;
        if let Ok(allocation) = self.try_allocate(mask, requirements.size, requirements.alignment) {
            return Ok(allocation);
        }

        Err(OutOfMemoryError {})
    }

    fn try_allocate(&self, memory_type_mask: MemoryTypeMask, size: u64, alignment: u64) -> Result<MemoryAllocation<B::Heap>, OutOfMemoryError> {
        let memory_types = unsafe { self.device.memory_type_infos() };
        if memory_type_mask == 0 {
            return Err(OutOfMemoryError {});
        }

        for i in 0..memory_types.len() {
            if ((1u32 << i as u32) & memory_type_mask) == 0 {
                continue;
            }

            if let Ok(allocation) = self.allocate_by_memory_type(i as u32, size, alignment) {
                return Ok(allocation);
            }
        }
        Err(OutOfMemoryError {})
    }

    pub(super) fn find_memory_type_mask(&self, usage: MemoryUsage, strictness: MemoryTypeMatchingStrictness) -> MemoryTypeMask {
        let memory_types = unsafe { self.device.memory_type_infos() };

        let mut mask = 0u32;
        for i in 0..memory_types.len() {
            let memory_type = &memory_types[i];
            let memory_kind = if usage == MemoryUsage::GPUMemory || usage == MemoryUsage::MappableGPUMemory { MemoryKind::VRAM } else { MemoryKind::RAM };
            let cpu_accessible = usage != MemoryUsage::GPUMemory;
            let cached = usage == MemoryUsage::MainMemoryCached;

            match strictness {
                MemoryTypeMatchingStrictness::ForceCoherent => {
                    if memory_type.is_cached != cached || (!self.is_uma && memory_type.is_cpu_accessible != cpu_accessible) || memory_type.memory_kind != memory_kind || (cpu_accessible && !memory_type.is_coherent) {
                        continue;
                    }
                }
                MemoryTypeMatchingStrictness::Normal => {
                    if memory_type.is_cached != cached || (!self.is_uma && memory_type.is_cpu_accessible != cpu_accessible) || memory_type.memory_kind != memory_kind {
                        continue;
                    }
                }
                MemoryTypeMatchingStrictness::Fallback => {
                    if (!memory_type.is_cached && cached) || (!memory_type.is_cpu_accessible && cpu_accessible) {
                        continue;
                    }
                }
            }
            mask |= 1 << i as u32;
        }
        return mask;
    }

    pub(super) fn is_uma(&self) -> bool {
        self.is_uma
    }

    pub fn cleanup_unused(&self) {
        let mut guard = self.inner.lock().unwrap();
        for (memory_type, chunks) in guard.chunks.iter_mut() {
            let mut retained_empty = 0u32;
            let chunks_count_before = chunks.len();
            chunks.retain(|b| {
                if !b.is_empty() {
                    return true;
                }
                retained_empty += 1;
                retained_empty < 2
            });
            if chunks.len() != chunks_count_before {
                trace!("Freed {} memory chunks in memory_type {}", chunks_count_before - chunks.len(), memory_type);
            }
        }
    }
}
