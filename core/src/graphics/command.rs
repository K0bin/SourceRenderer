use std::rc::Rc;
use std::sync::Arc;
use std::cell::RefCell;
use std::sync::Mutex;

use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;

use crate::graphics::RenderpassRecordingMode;
use graphics::{Backend, PipelineInfo};
use pool::Recyclable;

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

pub trait CommandPool<B: Backend> {
  fn get_command_buffer(&mut self, command_buffer_type: CommandBufferType) -> B::CommandBuffer;
}

pub trait CommandBuffer<B: Backend> {
  fn finish(self) -> B::CommandBufferSubmission;
  fn set_pipeline(&mut self, info: &PipelineInfo<B>);
  fn set_vertex_buffer(&mut self, vertex_buffer: Arc<B::Buffer>);
  fn set_index_buffer(&mut self, index_buffer: Arc<B::Buffer>);
  fn set_viewports(&mut self, viewports: &[ Viewport ]);
  fn set_scissors(&mut self, scissors: &[ Scissor ]);
  fn init_texture_mip_level(&mut self, src_buffer: &Arc<B::Buffer>, texture: &Arc<B::Texture>, mip_level: u32, array_layer: u32);
  fn draw(&mut self, vertices: u32, offset: u32);
  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32);
  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &B::TextureShaderResourceView);
  fn finish_binding(&mut self);
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum BindingFrequency {
  PerDraw,
  PerMaterial,
  PerModel,
  Rarely
}
