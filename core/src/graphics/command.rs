use std::rc::Rc;
use std::sync::Arc;
use std::cell::{RefCell, RefMut, Ref};
use std::sync::Mutex;

use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;

use crate::graphics::RenderpassRecordingMode;
use graphics::{Backend, PipelineInfo2};
use pool::Recyclable;
use std::ops::Deref;

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

pub trait CommandPool<B: Backend> : Send {
  fn get_command_buffer(&self, command_buffer_type: CommandBufferType) -> B::CommandBuffer;
}

pub trait Submission : Send {

}

pub trait CommandBuffer<B: Backend> {
  fn finish(self) -> B::Submission;
  fn set_pipeline(&mut self, pipeline: Arc<B::Pipeline>);
  fn set_pipeline2(&mut self, pipeline: &PipelineInfo2<B>);
  fn begin_render_pass(&mut self, renderpass: &B::RenderPass, recording_mode: RenderpassRecordingMode);
  fn end_render_pass(&mut self);
  fn set_vertex_buffer(&mut self, vertex_buffer: Arc<B::Buffer>);
  fn set_viewports(&mut self, viewports: &[ Viewport ]);
  fn set_scissors(&mut self, scissors: &[ Scissor ]);
  fn draw(&mut self, vertices: u32, offset: u32);
}
