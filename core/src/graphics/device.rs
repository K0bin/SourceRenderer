use std::sync::Arc;

use crate::graphics::{TextureInfo, TextureViewInfo, BufferUsage, GraphicsPipelineInfo, ShaderType, Backend};

use super::{RenderPassInfo, buffer::BufferInfo, texture::SamplerInfo, AccelerationStructureSizes, BottomLevelAccelerationStructureInfo, TopLevelAccelerationStructureInfo, rt::RayTracingPipelineInfo};

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum AdapterType {
  Discrete,
  Integrated,
  Virtual,
  Software,
  Other
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryUsage {
  VRAM,
  MappableVRAM,
  UncachedRAM,
  CachedRAM
}

pub trait Adapter<B: Backend> {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(&self, surface: &Arc<B::Surface>) -> B::Device;
}

pub const WHOLE_BUFFER: usize = usize::MAX;

pub trait Device<B: Backend> {
  fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<B::Buffer>;
  fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer> where T: 'static + Send + Sync + Sized + Clone;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &[u8], name: Option<&str>) -> Arc<B::Shader>;
  fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Arc<B::Texture>;
  fn create_sampling_view(&self, texture: &Arc<B::Texture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<B::TextureSamplingView>;
  fn create_render_target_view(&self, texture: &Arc<B::Texture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<B::TextureRenderTargetView>;
  fn create_storage_view(&self, texture: &Arc<B::Texture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<B::TextureStorageView>;
  fn create_depth_stencil_view(&self, texture: &Arc<B::Texture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<B::TextureDepthStencilView>;
  fn create_compute_pipeline(&self, shader: &Arc<B::Shader>, name: Option<&str>) -> Arc<B::ComputePipeline>;
  fn create_sampler(&self, info: &SamplerInfo) -> Arc<B::Sampler>;
  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<B>, renderpass_info: &RenderPassInfo, subpass: u32, name: Option<&str>) -> Arc<B::GraphicsPipeline>;
  fn wait_for_idle(&self);
  fn init_texture(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32, buffer_offset: usize);
  fn init_texture_async(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32, buffer_offset: usize) -> Option<Arc<B::Fence>>;
  fn init_buffer(&self, src_buffer: &Arc<B::Buffer>, dst_buffer: &Arc<B::Buffer>, src_offset: usize, dst_offset: usize, length: usize);
  fn flush_transfers(&self);
  fn free_completed_transfers(&self);
  fn create_fence(&self) -> Arc<B::Fence>;
  fn create_semaphore(&self) -> Arc<B::Semaphore>;
  fn graphics_queue(&self) -> &Arc<B::Queue>;
  fn prerendered_frames(&self) -> u32;
  fn supports_bindless(&self) -> bool;
  fn supports_ray_tracing(&self) -> bool;
  fn supports_indirect(&self) -> bool;
  fn supports_min_max_filter(&self) -> bool;
  fn supports_barycentrics(&self) -> bool; // TODO turn into flags
  fn insert_texture_into_bindless_heap(&self, texture: &Arc<B::TextureSamplingView>) -> u32;
  fn get_bottom_level_acceleration_structure_size(&self, info: &BottomLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
  fn get_top_level_acceleration_structure_size(&self, info: &TopLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
  fn create_raytracing_pipeline(&self, info: &RayTracingPipelineInfo<B>) -> Arc<B::RayTracingPipeline>;
}
