use crate::Matrix4;

use super::*;

#[derive(Debug)]
pub struct AccelerationStructureSizes {
  pub size: u64,
  pub build_scratch_size: u64,
  pub update_scratch_size: u64
}

pub struct BottomLevelAccelerationStructureInfo<'a, B: GPUBackend> {
  pub vertex_position_offset: u32,
  pub vertex_stride: u32,
  pub vertex_format: Format,
  pub vertex_buffer: &'a B::Buffer,
  pub vertex_buffer_offset: u64,
  pub index_format: IndexFormat,
  pub index_buffer: &'a B::Buffer,
  pub index_buffer_offset: u64,
  pub opaque: bool,
  pub mesh_parts: &'a [AccelerationStructureMeshRange],
  pub max_vertex: u32,
}
#[derive(Clone)]
pub struct AccelerationStructureMeshRange {
  pub primitive_start: u32,
  pub primitive_count: u32
}

pub struct TopLevelAccelerationStructureInfo<'a, B: GPUBackend> {
  pub instances_buffer: &'a B::Buffer,
  pub instances_buffer_offset: u64,
  pub instances_count: u32
}

pub struct AccelerationStructureInstance<'a, B: GPUBackend> {
  pub acceleration_structure: &'a B::AccelerationStructure,
  pub transform: Matrix4,
  pub front_face: FrontFace,
  pub id: u32
}

pub trait AccelerationStructure {}

pub struct RayTracingPipelineInfo<'a, B: GPUBackend> {
  pub ray_gen_shader: &'a B::Shader,
  pub closest_hit_shaders: &'a [&'a B::Shader],
  pub any_hit_shaders: &'a [&'a B::Shader],
  pub miss_shaders: &'a [&'a B::Shader],
}
