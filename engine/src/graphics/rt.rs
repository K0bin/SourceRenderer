use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::{gpu::*, Matrix4};

use super::*;

pub struct BottomLevelAccelerationStructureInfo<'a> {
    pub vertex_position_offset: u32,
    pub vertex_stride: u32,
    pub vertex_format: Format,
    pub vertex_buffer: &'a Arc<BufferSlice>,
    pub vertex_buffer_offset: usize,
    pub index_format: IndexFormat,
    pub index_buffer: &'a Arc<BufferSlice>,
    pub index_buffer_offset: usize,
    pub opaque: bool,
    pub mesh_parts: &'a [AccelerationStructureMeshRange],
    pub max_vertex: u32,
}

pub struct TopLevelAccelerationStructureInfo<'a> {
    pub instances: &'a [AccelerationStructureInstance<'a>],
}

pub struct AccelerationStructureInstance<'a> {
    pub acceleration_structure: &'a Arc<AccelerationStructure>,
    pub transform: Matrix4,
    pub front_face: FrontFace,
    pub id: u32
}

pub use sourcerenderer_core::gpu::AccelerationStructureMeshRange;

pub struct AccelerationStructure {
    acceleration_structure: ManuallyDrop<active_gpu_backend::AccelerationStructure>,
    buffer: ManuallyDrop<Arc<BufferSlice>>,
    destroyer: Arc<DeferredDestroyer>
}

impl AccelerationStructure {
    pub(super) fn new(acceleration_structure: active_gpu_backend::AccelerationStructure, buffer: Arc<BufferSlice>, destroyer: &Arc<DeferredDestroyer>) -> Self {
        Self {
            acceleration_structure: ManuallyDrop::new(acceleration_structure),
            buffer: ManuallyDrop::new(buffer),
            destroyer: destroyer.clone()
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::AccelerationStructure {
        &self.acceleration_structure
    }

    #[allow(unused)]
    #[inline(always)]
    pub(super) fn buffer(&self) -> &Arc<BufferSlice> {
        &self.buffer
    }
}

impl Drop for AccelerationStructure {
    fn drop(&mut self) {
        let acceleration_structure = unsafe { ManuallyDrop::take(&mut self.acceleration_structure) };
        self.destroyer.destroy_acceleration_structure(acceleration_structure);
        unsafe { ManuallyDrop::drop(&mut self.buffer) };
    }
}
