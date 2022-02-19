use std::sync::Arc;

use super::{Format, IndexFormat, Backend};

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
}
#[derive(Clone)]
pub struct AccelerationStructureMeshRange {
  pub primitive_start: u32,
  pub primitive_count: u32
}

pub trait AccelerationStructure {}
