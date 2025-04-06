use std::ffi::c_void;

use crate::Vec3UI;

use super::*;

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AdapterType {
  Discrete,
  Integrated,
  Virtual,
  Software,
  Other
}

pub trait Adapter<B: GPUBackend> {
  fn adapter_type(&self) -> AdapterType;
  unsafe fn create_device(&self, surface: &B::Surface) -> B::Device;
}

pub const WHOLE_BUFFER: u64 = u64::MAX;

pub const PER_SET_BINDINGS: u32 = 32;
pub const NON_BINDLESS_SET_COUNT: u32 = 3;
pub const TOTAL_SET_COUNT: u32 = NON_BINDLESS_SET_COUNT + 1;
pub const BINDLESS_TEXTURE_SET_INDEX: u32 = NON_BINDLESS_SET_COUNT;
pub const BINDLESS_TEXTURE_COUNT: u32 = 500_000;

#[derive(Debug)]
pub struct MemoryTextureCopyRegion {
  pub row_pitch: u64,
  pub slice_pitch: u64,
  pub texture_subresource: TextureSubresource,
  pub texture_offset: Vec3UI,
  pub texture_extent: Vec3UI,
}

pub struct CPUTextureTransition<'a, B: GPUBackend> {
  pub old_layout: TextureLayout,
  pub new_layout: TextureLayout,
  pub texture: &'a B::Texture,
  pub range: BarrierTextureRange,
}

pub trait Device<B: GPUBackend> {
  unsafe fn create_buffer(&self, info: &BufferInfo, memory_type_index: u32, name: Option<&str>) -> Result<B::Buffer, OutOfMemoryError>;
  unsafe fn create_texture(&self, info: &TextureInfo, memory_type_index: u32, name: Option<&str>) -> Result<B::Texture, OutOfMemoryError>;
  unsafe fn create_shader(&self, shader: &PackedShader, name: Option<&str>) -> B::Shader;
  unsafe fn create_texture_view(&self, texture: &B::Texture, info: &TextureViewInfo, name: Option<&str>) -> B::TextureView;
  unsafe fn create_compute_pipeline(&self, shader: &B::Shader, name: Option<&str>) -> B::ComputePipeline;
  unsafe fn create_sampler(&self, info: &SamplerInfo) -> B::Sampler;
  unsafe fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<B>, name: Option<&str>) -> B::GraphicsPipeline;
  unsafe fn create_mesh_graphics_pipeline(&self, info: &MeshGraphicsPipelineInfo<B>, name: Option<&str>) -> B::MeshGraphicsPipeline;
  unsafe fn wait_for_idle(&self);
  unsafe fn create_fence(&self, is_cpu_accessible: bool) -> B::Fence;
  unsafe fn memory_infos(&self) -> Vec<MemoryInfo>;
  unsafe fn memory_type_infos(&self) -> &[MemoryTypeInfo];
  unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<B::Heap, OutOfMemoryError>;
  unsafe fn get_buffer_heap_info(&self, info: &BufferInfo) -> ResourceHeapInfo;
  unsafe fn get_texture_heap_info(&self, info: &TextureInfo) -> ResourceHeapInfo;
  unsafe fn insert_texture_into_bindless_heap(&self, slot: u32, texture: &B::TextureView);
  unsafe fn copy_to_texture(&self, src: *const c_void, dst: &B::Texture, texture_layout: TextureLayout, region: &MemoryTextureCopyRegion);
  unsafe fn transition_texture(&self, dst: &B::Texture, transition: &CPUTextureTransition<'_, B>);
  fn graphics_queue(&self) -> &B::Queue;
  fn compute_queue(&self) -> Option<&B::Queue>;
  fn transfer_queue(&self) -> Option<&B::Queue>;
  fn supports_bindless(&self) -> bool;
  fn supports_ray_tracing_pipeline(&self) -> bool;
  fn supports_ray_tracing_query(&self) -> bool;
  fn supports_indirect(&self) -> bool;
  fn supports_min_max_filter(&self) -> bool;
  fn supports_barycentrics(&self) -> bool;
  fn supports_mesh_shader(&self) -> bool; // TODO turn into flags
  unsafe fn get_bottom_level_acceleration_structure_size(&self, info: &BottomLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
  unsafe fn get_top_level_acceleration_structure_size(&self, info: &TopLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
  fn get_top_level_instances_buffer_size(&self, instances: &[AccelerationStructureInstance<B>]) -> u64;
  unsafe fn get_raytracing_pipeline_sbt_buffer_size(&self, info: &RayTracingPipelineInfo<B>) -> u64;
  unsafe fn create_raytracing_pipeline(&self, info: &RayTracingPipelineInfo<B>, sbt_buffer: &B::Buffer, sbt_buffer_offset: u64, name: Option<&str>) -> B::RayTracingPipeline;
  unsafe fn create_query_pool(&self, count: u32) -> B::QueryPool;
}
