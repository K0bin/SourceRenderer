use std::sync::Arc;

use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;

use crate::graphics::{Backend, BufferUsage};

use super::ShaderType;

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

#[derive(Clone, Debug, Copy, PartialEq, Hash)]
pub enum CommandBufferType {
  PRIMARY,
  SECONDARY
}

#[derive(Clone)]
pub enum PipelineBinding<'a, B: Backend> {
  Graphics(&'a Arc<B::GraphicsPipeline>),
  Compute(&'a Arc<B::ComputePipeline>)
}

pub trait CommandBuffer<B: Backend> {
  fn set_pipeline(&mut self, pipeline: PipelineBinding<B>);
  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<B::Buffer>);
  fn set_index_buffer(&mut self, index_buffer: &Arc<B::Buffer>);
  fn set_viewports(&mut self, viewports: &[ Viewport ]);
  fn set_scissors(&mut self, scissors: &[ Scissor ]);
  fn init_texture_mip_level(&mut self, src_buffer: &Arc<B::Buffer>, texture: &Arc<B::Texture>, mip_level: u32, array_layer: u32);
  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<B::Buffer>
  where T: 'static + Send + Sync + Sized + Clone;
  fn upload_dynamic_data_inline<T>(&mut self, data: &[T], visible_for_shader_stage: ShaderType)
    where T: 'static + Send + Sync + Sized + Clone;
  fn draw(&mut self, vertices: u32, offset: u32);
  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32);
  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<B::TextureShaderResourceView>);
  fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<B::Buffer>);
  fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<B::Buffer>);
  fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<B::TextureUnorderedAccessView>);
  fn finish_binding(&mut self);
  fn begin_label(&mut self, label: &str);
  fn end_label(&mut self);
  fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32);
  fn blit(&mut self, src_texture: &Arc<B::Texture>, src_array_layer: u32, src_mip_level: u32, dst_texture: &Arc<B::Texture>, dst_array_layer: u32, dst_mip_level: u32);
  fn finish(self) -> B::CommandBufferSubmission;
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub enum BindingFrequency {
  PerDraw = 0,
  PerMaterial = 1,
  PerFrame = 2,
  Rarely = 3
}

pub trait InnerCommandBufferProvider<B: Backend> : Send + Sync {
  fn get_inner_command_buffer(&self) -> B::CommandBuffer;
}
