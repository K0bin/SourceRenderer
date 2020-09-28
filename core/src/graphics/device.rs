use std::sync::Arc;
use std::rc::Rc;

use crate::graphics::{Surface, TextureInfo, TextureShaderResourceViewInfo};
use crate::graphics::Buffer;
use crate::graphics::BufferUsage;
use crate::graphics::PipelineInfo;
use crate::graphics::Shader;
use crate::graphics::ShaderType;
use crate::graphics::Texture;
use crate::graphics::Backend;

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
  fn create_device(&self, surface: &B::Surface) -> B::Device;
}

pub trait Device<B: Backend> {
  fn create_buffer(&self, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer>;
  fn upload_data<T>(&self, data: &T, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer> where T: 'static + Send + Sync + Sized + Clone;
  fn upload_data_slice<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer> where T: 'static + Send + Sync + Sized + Clone;
  fn upload_data_raw(&self, data: &[u8], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer>;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> Arc<B::Shader>;
  fn create_texture(&self, info: &TextureInfo) -> Arc<B::Texture>;
  fn create_shader_resource_view(&self, texture: &Arc<B::Texture>, info: &TextureShaderResourceViewInfo) -> Arc<B::TextureShaderResourceView>;
  fn wait_for_idle(&self);

  fn create_render_graph_template(&self, info: &crate::graphics::RenderGraphTemplateInfo) -> B::RenderGraphTemplate;
  fn create_render_graph(&self, template: &Arc<B::RenderGraphTemplate>, info: &crate::graphics::graph::RenderGraphInfo<B>, swapchain: &Arc<B::Swapchain>) -> B::RenderGraph;
  fn init_texture(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32) -> Arc<B::Fence>;
  fn init_buffer(&self, src_buffer: &Arc<B::Buffer>, dst_buffer: &Arc<B::Buffer>) -> Arc<B::Fence>;
  fn flush_transfers(&self);
  fn free_completed_transfers(&self);
}
