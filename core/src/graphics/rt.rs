use std::sync::Arc;

use crate::Matrix4;

use super::{Format, IndexFormat, Backend, FrontFace};

pub struct AccelerationStructureSizes {
  pub size: u64,
  pub build_scratch_size: u64,
  pub update_scratch_size: u64
}

pub struct BottomLevelAccelerationStructureInfo<'a, B: Backend> {
  pub vertex_position_offset: u32,
  pub vertex_stride: u32,
  pub vertex_format: Format,
  pub vertex_buffer: &'a Arc<B::Buffer>,
  pub index_format: IndexFormat,
  pub index_buffer: &'a Arc<B::Buffer>,
  pub opaque: bool,
  pub mesh_parts: &'a [AccelerationStructureMeshRange],
  pub max_vertex: u32,
}
#[derive(Clone)]
pub struct AccelerationStructureMeshRange {
  pub primitive_start: u32,
  pub primitive_count: u32
}

pub struct TopLevelAccelerationStructureInfo<'a, B: Backend> {
  pub instances_buffer: &'a Arc<B::Buffer>,
  pub instances: &'a [AccelerationStructureInstance<'a, B>],
}

pub struct AccelerationStructureInstance<'a, B: Backend> {
  pub acceleration_structure: &'a Arc<B::AccelerationStructure>,
  pub transform: Matrix4,
  pub front_face: FrontFace,
}

pub trait AccelerationStructure {}
