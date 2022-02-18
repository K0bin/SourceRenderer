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
  pub primitive_count: u32,
  pub opaque: bool,
}

pub trait AccelerationStructure {}
