use std::rc::Rc;
use std::sync::Arc;

use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;

use crate::graphics::RenderPass;
use crate::graphics::Buffer;
use crate::graphics::RenderpassRecordingMode;
use crate::graphics::Pipeline;

pub struct Viewport {
  pub position: Vec2,
  pub extent: Vec2,
  pub min_depth: f32,
  pub max_depth: f32
}

pub struct Scissor {
  pub position: Vec2I,
  pub extent: Vec2UI
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum CommandBufferType {
  PRIMARY,
  SECONDARY
}

pub trait CommandPool {
  fn create_command_buffer(self: Rc<Self>, command_buffer_type: CommandBufferType) -> Rc<dyn CommandBuffer>;
  fn reset(&self);
}

pub trait CommandBuffer {
  fn begin(&self);
  fn end(&self);
  fn set_pipeline(&self, pipeline: Arc<dyn Pipeline>);
  fn begin_render_pass(&self, renderpass: &dyn RenderPass, recording_mode: RenderpassRecordingMode);
  fn end_render_pass(&self);
  fn set_vertex_buffer(&self, vertex_buffer: &dyn Buffer);
  fn set_viewports(&self, viewports: &[ Viewport ]);
  fn set_scissors(&self, scissors: &[ Scissor ]);
  fn draw(&self, vertices: u32, offset: u32);
}
