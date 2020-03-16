use std::sync::Arc;
use std::rc::Rc;

use graphics::Surface;
use graphics::CommandPool;
use graphics::Buffer;
use graphics::BufferUsage;
use graphics::Pipeline;
use graphics::PipelineInfo;
use graphics::Shader;
use graphics::ShaderType;
use graphics::Texture;
use graphics::RenderTargetView;
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
  fn create_buffer(&self, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> B::Buffer;
  fn upload_data<T>(&self, data: T) -> B::Buffer;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> B::Shader;
  fn create_render_target_view(&self, texture: Arc<B::Texture>) -> B::RenderTargetView;
  fn wait_for_idle(&self);

  fn create_render_graph(&self, graph_info: &crate::graphics::graph::RenderGraphInfo<B>, swapchin: &Arc<B::Swapchain>) -> B::RenderGraph;
}
