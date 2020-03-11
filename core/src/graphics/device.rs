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

pub trait Adapter<B: Backend> : Send + Sync {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(&self, surface: &B::Surface) -> B::Device;
}

pub trait Device<B: Backend> : Send + Sync {
  fn get_queue(&self, queue_type: QueueType) -> Option<&B::Queue>;
  fn create_buffer(&self, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> B::Buffer;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> B::Shader;
  fn create_pipeline(&self, info: &PipelineInfo<B>) -> B::Pipeline;
  fn create_renderpass_layout(&self, info: &RenderPassLayoutInfo) -> B::RenderPassLayout;
  fn create_renderpass(&self, info: &RenderPassInfo<B>) -> B::RenderPass;
  fn create_render_target_view(&self, texture: Arc<B::Texture>) -> B::RenderTargetView;
  fn create_semaphore(&self) -> B::Semaphore;
  fn create_fence(&self) -> B::Fence;
  fn wait_for_idle(&self);

  fn create_render_graph(self: Arc<Self>, graph_info: &crate::graphics::graph::RenderGraphInfo, swapchin: &B::Swapchain) -> B::RenderGraph;
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum QueueType {
  Graphics,
  Compute,
  Transfer
}

// TODO change submit & present to require a mutable reference
pub trait Queue<B: Backend> : Send + Sync {
  fn create_command_pool(&self) -> B::CommandPool;
  fn get_queue_type(&self) -> QueueType;
  fn supports_presentation(&self) -> bool;
  fn submit(&self, submission: &B::Submission, fence: Option<&B::Fence>, wait_semaphore: &[ &B::Semaphore ], signal_semaphore: &[ &B::Semaphore ]);
  fn present(&self, swapchain: &B::Swapchain, image_index: u32, wait_semaphores: &[ &B::Semaphore ]);
}
