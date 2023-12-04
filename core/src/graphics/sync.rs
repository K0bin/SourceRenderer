use std::sync::Arc;

use super::Backend;

pub trait Fence {
  fn value(&self) -> u64;
  fn await_value(&self, value: u64);
}

pub struct FenceValuePair<B: Backend> {
  pub fence: Arc<B::Fence>,
  pub value: u64
}

impl<B: Backend> FenceValuePair<B> {
  pub fn is_signalled(&self) -> bool {
    self.fence.value() >= self.value
  }
}

impl<B: Backend> Clone for FenceValuePair<B> {
  fn clone(&self) -> Self {
      Self {
        fence: self.fence.clone(),
        value: self.value
      }
  }
}
