use std::sync::Arc;
use graphics::{Backend, PipelineInfo, Viewport, Scissor};

pub trait Context<B: Backend> {
  fn set_pipeline(&mut self, info: &PipelineInfo<B>);
  fn set_vertex_buffer(&mut self, vertex_buffer: Arc<B::Buffer>);
  fn set_viewports(&mut self, viewports: &[ Viewport ]);
  fn set_scissors(&mut self, scissors: &[ Scissor ]);
  fn draw(&mut self, vertices: u32, offset: u32);
}
