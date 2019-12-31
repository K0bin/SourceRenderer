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
use graphics::RenderPassLayout;
use graphics::RenderPassLayoutInfo;
use graphics::RenderPass;
use graphics::RenderPassInfo;
use graphics::Texture;
use graphics::RenderTargetView;
use graphics::Backend;

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum AdapterType {
  Discrete,
  Integrated,
  Virtual,
  Software,
  Other
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum MemoryUsage {
  GpuOnly,
  CpuOnly,
  CpuToGpu,
  GpuToCpu
}

pub trait Adapter<B: Backend> {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(self: Arc<Self>, surface: Arc<B::Surface>) -> Arc<B::Device>;
}

pub trait Device<B: Backend> {
  fn create_queue(self: Arc<Self>, queue_type: QueueType) -> Option<Arc<B::Queue>>;
  fn create_buffer(self: Arc<Self>, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<B::Buffer>;
  fn create_shader(self: Arc<Self>, shader_type: ShaderType, bytecode: &Vec<u8>) -> Arc<B::Shader>;
  fn create_pipeline(self: Arc<Self>, info: &PipelineInfo<B>) -> Arc<B::Pipeline>;
  fn create_renderpass_layout(self: Arc<Self>, info: &RenderPassLayoutInfo) -> Arc<B::RenderPassLayout>;
  fn create_renderpass(self: Arc<Self>, info: &RenderPassInfo<B>) -> Arc<B::RenderPass>;
  fn create_render_target_view(self: Arc<Self>, texture: Arc<B::Texture>) -> Arc<B::RenderTargetView>;
  fn wait_for_idle(&self);
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum QueueType {
  Graphics,
  Compute,
  Transfer
}

pub trait Queue<B: Backend> {
  fn create_command_pool(self: Arc<Self>) -> B::CommandPool;
  fn get_queue_type(&self) -> QueueType;
  fn supports_presentation(&self) -> bool;
  fn submit(&self, command_buffer: &B::CommandBuffer);
  fn present(&self, swapchain: &B::Swapchain, image_index: u32);
}
