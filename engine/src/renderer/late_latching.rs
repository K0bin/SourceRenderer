use std::sync::Arc;

use sourcerenderer_core::graphics::Backend;

use crate::input::InputState;

use super::drawable::View;

pub trait LateLatching<B: Backend> : Send + Sync {
  fn buffer(&self) -> Arc<B::Buffer>;
  fn history_buffer(&self) -> Option<Arc<B::Buffer>>;
  fn before_recording(&self, input: &InputState, view: &View);
  fn before_submit(&self, input: &InputState, view: &View);
  fn after_submit(&self, device: &B::Device);
}
