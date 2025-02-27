use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::{gpu::*, Matrix4};

use super::*;

pub struct BottomLevelAccelerationStructureInfo<'a, B: GPUBackend> {
    pub vertex_position_offset: u32,
    pub vertex_stride: u32,
    pub vertex_format: Format,
    pub vertex_buffer: &'a Arc<BufferSlice<B>>,
    pub vertex_buffer_offset: usize,
    pub index_format: IndexFormat,
    pub index_buffer: &'a Arc<BufferSlice<B>>,
    pub index_buffer_offset: usize,
    pub opaque: bool,
    pub mesh_parts: &'a [AccelerationStructureMeshRange],
    pub max_vertex: u32,
}

pub struct TopLevelAccelerationStructureInfo<'a, B: GPUBackend> {
    pub instances: &'a [AccelerationStructureInstance<'a, B>],
}

pub struct AccelerationStructureInstance<'a, B: GPUBackend> {
    pub acceleration_structure: &'a Arc<AccelerationStructure<B>>,
    pub transform: Matrix4,
    pub front_face: FrontFace,
    pub id: u32
}

pub use sourcerenderer_core::gpu::AccelerationStructureMeshRange;

pub struct AccelerationStructure<B: GPUBackend> {
    acceleration_structure: ManuallyDrop<B::AccelerationStructure>,
    buffer: ManuallyDrop<Arc<BufferSlice<B>>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> AccelerationStructure<B> {
    pub(super) fn new(acceleration_structure: B::AccelerationStructure, buffer: Arc<BufferSlice<B>>, destroyer: &Arc<DeferredDestroyer<B>>) -> Self {
        Self {
            acceleration_structure: ManuallyDrop::new(acceleration_structure),
            buffer: ManuallyDrop::new(buffer),
            destroyer: destroyer.clone()
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &B::AccelerationStructure {
        &self.acceleration_structure
    }

    #[allow(unused)]
    #[inline(always)]
    pub(super) fn buffer(&self) -> &Arc<BufferSlice<B>> {
        &self.buffer
    }
}

impl<B: GPUBackend> Drop for AccelerationStructure<B> {
    fn drop(&mut self) {
        let acceleration_structure = unsafe { ManuallyDrop::take(&mut self.acceleration_structure) };
        let buffer = unsafe { ManuallyDrop::take(&mut self.buffer) };
        self.destroyer.destroy_acceleration_structure(acceleration_structure);
        self.destroyer.destroy_buffer_slice_ref(buffer);
    }
}
