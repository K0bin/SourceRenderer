use std::sync::Arc;

use crate::graphics::Backend;

pub trait Instance<B: Backend> {
  fn list_adapters(self: Arc<Self>) -> Vec<Arc<B::Adapter>>;
}
