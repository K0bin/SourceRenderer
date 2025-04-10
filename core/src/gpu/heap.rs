use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DedicatedAllocationPreference {
    RequireDedicated,
    PreferDedicated,
    DontCare,
    PreferSuballocated,
    RequireSuballocated
}

#[derive(Debug)]
pub struct ResourceHeapInfo {
    pub dedicated_allocation_preference: DedicatedAllocationPreference,
    pub memory_type_mask: u32,
    pub alignment: u64,
    pub size: u64
}

#[derive(Debug)]
pub struct MemoryTypeInfo {
    pub memory_index: u32,
    pub memory_kind: MemoryKind,
    pub is_cached: bool,
    pub is_cpu_accessible: bool,
    pub is_coherent: bool
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MemoryKind {
    VRAM,
    RAM
}

#[derive(Debug)]
pub struct MemoryInfo {
    pub available: u64,
    pub total: u64,
    pub memory_kind: MemoryKind
}

#[derive(Debug)]
pub struct OutOfMemoryError {}

pub trait Heap<B: GPUBackend> {
    fn memory_type_index(&self) -> u32;
    unsafe fn create_buffer(&self, info: &BufferInfo, offset: u64, name: Option<&str>) -> Result<B::Buffer, OutOfMemoryError>;
    unsafe fn create_texture(&self, info: &TextureInfo, offset: u64, name: Option<&str>) -> Result<B::Texture, OutOfMemoryError>;
}
