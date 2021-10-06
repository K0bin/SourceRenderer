use std::sync::Arc;

use sourcerenderer_core::{Vec3, graphics::Backend};

use crate::input::InputState;

use super::drawable::View;

pub trait LateLatching<B: Backend> : Send + Sync {
  fn buffer(&self) -> Arc<B::Buffer>;
  fn process_input(&self, input: &InputState);
  fn before_submit(&self, input: &InputState, view: &View);
}
