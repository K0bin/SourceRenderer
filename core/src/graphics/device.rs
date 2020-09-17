use std::sync::Arc;
use std::rc::Rc;

use graphics::{Surface, TextureInfo, TextureShaderResourceViewInfo};
use graphics::Buffer;
use graphics::BufferUsage;
use graphics::PipelineInfo;
use graphics::Shader;
use graphics::ShaderType;
use graphics::Texture;
use graphics::Backend;

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum AdapterType {
  Discrete,
  Integrated,
  Virtual,
  Software,
  Other
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
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

  fn create_render_graph(&self, graph_info: &crate::graphics::graph::RenderGraphInfo<B>, swapchin: &Arc<B::Swapchain>) -> B::RenderGraph;
  fn init_texture(&self, texture: &Arc<B::Texture>, buffer: &Arc<B::Buffer>, mip_level: u32, array_layer: u32) -> Arc<B::Fence>;
  fn init_buffer(&self, src_buffer: &Arc<B::Buffer>, dst_buffer: &Arc<B::Buffer>) -> Arc<B::Fence>;
  fn flush_transfers(&self);
  fn free_completed_transfers(&self);
}
