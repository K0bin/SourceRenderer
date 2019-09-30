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

pub trait Adapter {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(self: Arc<Self>, surface: Arc<dyn Surface>, ) -> Arc<dyn Device>;
}

pub trait Device {
  fn create_queue(self: Arc<Self>, queue_type: QueueType) -> Option<Arc<dyn Queue>>;
  fn create_buffer(self: Arc<Self>, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<dyn Buffer>;
  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> Arc<dyn Shader>;
  fn create_pipeline(self: Arc<Self>, info: &PipelineInfo) -> Arc<dyn Pipeline>;
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum QueueType {
  Graphics,
  Compute,
  Transfer
}

pub trait Queue {
  fn create_command_pool(self: Arc<Self>) -> Rc<dyn CommandPool>;
  fn get_queue_type(&self) -> QueueType;
  fn supports_presentation(&self) -> bool;
}
