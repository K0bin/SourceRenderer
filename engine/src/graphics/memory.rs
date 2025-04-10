use std::{collections::HashMap, sync::Arc};
use crate::Mutex;

use log::trace;
use super::gpu;

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

pub(super) struct MemoryAllocator {
    device: Arc<active_gpu_backend::Device>,
    is_uma: bool,
    inner: Mutex<MemoryAllocatorInner>
}

pub(super) struct MemoryAllocatorInner {
    chunks: HashMap<MemoryTypeIndex, Vec<Chunk<active_gpu_backend::Heap>>>
}

const CHUNK_SIZE: u64 = 256 << 20;

pub(super) struct MemoryAllocation<H: Send + Sync> {
    allocation: Allocation<H>,
    _memory_usage: MemoryUsage
}

impl<T: Send + Sync> AsRef<Allocation<T>> for MemoryAllocation<T> {
    fn as_ref(&self) -> &Allocation<T> {
        &self.allocation
    }
}

#[derive(Debug)]
pub(super) enum MemoryTypeMatchingStrictness {
    Strict,
    Normal,
    Fallback
}

impl MemoryAllocator {
    pub(super) fn new(device: &Arc<active_gpu_backend::Device>) -> Self {
        let memory_types = unsafe { device.memory_type_infos() };
        let is_uma = memory_types.iter().all(|memory_type| memory_type.memory_kind == gpu::MemoryKind::VRAM);

        Self {
            device: device.clone(),
            is_uma,
            inner: Mutex::new(MemoryAllocatorInner {
                chunks: HashMap::new()
            })
        }
    }

    fn allocate_by_memory_type(&self, memory_type_index: MemoryTypeIndex, size: u64, alignment: u64) -> Result<MemoryAllocation<active_gpu_backend::Heap>, OutOfMemoryError> {
        let mut inner = self.inner.lock().unwrap();
        let chunk_list = inner.chunks.entry(memory_type_index).or_insert(Vec::new());
        let allocation = chunk_list.iter().find_map(|chunk| chunk.allocate(size, alignment));
        if let Some(allocation) = allocation {
            return Ok(MemoryAllocation {
                allocation,
                _memory_usage: self.memory_usage(memory_type_index)
            });
        }

        let heap = unsafe { self.device.create_heap(memory_type_index, CHUNK_SIZE) };
        if heap.is_err() {
            return Err(OutOfMemoryError {  });
        }
        let heap = heap.unwrap();
        let chunk = Chunk::new(heap, CHUNK_SIZE.max(size));
        let allocation = chunk.allocate(size, alignment).unwrap();
        chunk_list.push(chunk);
        Ok(MemoryAllocation {
            allocation,
            _memory_usage: self.memory_usage(memory_type_index)
        })
    }

    pub(super) fn allocate(&self, usage: MemoryUsage, requirements: &gpu::ResourceHeapInfo) -> Result<MemoryAllocation<active_gpu_backend::Heap>, OutOfMemoryError> {
        let mut mask: u32;

        if usage != MemoryUsage::GPUMemory {
            mask = self.find_memory_type_mask(usage,  MemoryTypeMatchingStrictness::Strict) & requirements.memory_type_mask;
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

    fn try_allocate(&self, memory_type_mask: MemoryTypeMask, size: u64, alignment: u64) -> Result<MemoryAllocation<active_gpu_backend::Heap>, OutOfMemoryError> {
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
            let memory_kind = if !self.is_uma() && (usage == MemoryUsage::GPUMemory || usage == MemoryUsage::MappableGPUMemory) { gpu::MemoryKind::VRAM } else { gpu::MemoryKind::RAM };
            let cpu_accessible = usage != MemoryUsage::GPUMemory;
            let cached = usage == MemoryUsage::MainMemoryCached;

            match strictness {
                MemoryTypeMatchingStrictness::Strict => {
                    if (cached != memory_type.is_cached)
                        || cpu_accessible != !memory_type.is_cpu_accessible
                        || memory_type.memory_kind != memory_kind
                        || (cpu_accessible && !memory_type.is_coherent) {
                        continue;
                    }
                }
                MemoryTypeMatchingStrictness::Normal => {
                    if (cached && !memory_type.is_cached)
                        || (cpu_accessible && !memory_type.is_cpu_accessible)
                        || memory_type.memory_kind != memory_kind {
                        continue;
                    }
                }
                MemoryTypeMatchingStrictness::Fallback => {
                    if (cached && !memory_type.is_cached) || (cpu_accessible && !memory_type.is_cpu_accessible) {
                        continue;
                    }
                }
            }
            mask |= 1 << i as u32;
        }
        return mask;
    }

    #[inline(always)]
    pub(super) fn memory_type_info(&self, memory_type_index: u32) -> &gpu::MemoryTypeInfo {
        let memory_types = unsafe { self.device.memory_type_infos() };
        &memory_types[(memory_type_index as usize).min(memory_types.len() - 1)]
    }

    pub(super) fn memory_usage(&self, memory_type_index: u32) -> MemoryUsage {
        let memory_type_info = self.memory_type_info(memory_type_index);
        if memory_type_info.is_cpu_accessible {
            if memory_type_info.is_cached {
                MemoryUsage::MainMemoryCached
            } else if memory_type_info.memory_kind == gpu::MemoryKind::VRAM {
                MemoryUsage::MappableGPUMemory
            } else {
                MemoryUsage::MainMemoryWriteCombined
            }
        } else {
            MemoryUsage::GPUMemory
        }
    }

    #[inline(always)]
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
