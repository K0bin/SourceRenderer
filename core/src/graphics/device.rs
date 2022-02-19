use std::sync::Arc;

use crate::graphics::{TextureInfo, TextureShaderResourceViewInfo, BufferUsage, GraphicsPipelineInfo, ShaderType, Backend};

use super::{RenderPassInfo, TextureRenderTargetViewInfo, buffer::BufferInfo, texture::{SamplerInfo, TextureDepthStencilViewInfo, TextureUnorderedAccessViewInfo}, AccelerationStructureSizes, BottomLevelAccelerationStructureInfo, TopLevelAccelerationStructureInfo};

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
  GpuOnly,
  CpuOnly,
  CpuToGpu,
  GpuToCpu
}

pub trait Adapter<B: Backend> {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(&self, surface: &Arc<B::Surface>) -> B::Device;
}

pub trait Device<B: Backend> {
  fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<B::Buffer>;
  fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer> where T: 'static + Send + Sync + Sized + Clone;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &[u8], name: Option<&str>) -> Arc<B::Shader>;
  fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Arc<B::Texture>;
  fn create_shader_resource_view(&self, texture: &Arc<B::Texture>, info: &TextureShaderResourceViewInfo) -> Arc<B::TextureShaderResourceView>;
  fn create_render_target_view(&self, texture: &Arc<B::Texture>, info: &TextureRenderTargetViewInfo) -> Arc<B::TextureRenderTargetView>;
  fn create_unordered_access_view(&self, texture: &Arc<B::Texture>, info: &TextureUnorderedAccessViewInfo) -> Arc<B::TextureUnorderedAccessView>;
  fn create_depth_stencil_view(&self, texture: &Arc<B::Texture>, info: &TextureDepthStencilViewInfo) -> Arc<B::TextureDepthStencilView>;
  fn create_compute_pipeline(&self, shader: &Arc<B::Shader>) -> Arc<B::ComputePipeline>;
  fn create_sampler(&self, info: &SamplerInfo) -> Arc<B::Sampler>;
  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<B>, renderpass_info: &RenderPassInfo, subpass: u32) -> Arc<B::GraphicsPipeline>;
  fn wait_for_idle(&self);
  fn init_texture(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32);
  fn init_texture_async(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32) -> Option<Arc<B::Fence>>;
  fn init_buffer(&self, src_buffer: &Arc<B::Buffer>, dst_buffer: &Arc<B::Buffer>);
  fn flush_transfers(&self);
  fn free_completed_transfers(&self);
  fn create_fence(&self) -> Arc<B::Fence>;
  fn create_semaphore(&self) -> Arc<B::Semaphore>;
  fn graphics_queue(&self) -> &Arc<B::Queue>;
  fn prerendered_frames(&self) -> u32;
  fn supports_bindless(&self) -> bool;
  fn insert_texture_into_bindless_heap(&self, texture: &Arc<B::TextureShaderResourceView>) -> u32;
  fn get_bottom_level_acceleration_structure_size(&self, info: &BottomLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
  fn get_top_level_acceleration_structure_size(&self, info: &TopLevelAccelerationStructureInfo<B>) -> AccelerationStructureSizes;
}
