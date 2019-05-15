use std::sync::Arc;

use graphics::Adapter;

pub trait Instance {
  fn list_adapters(self: Arc<Self>) -> Vec<Arc<dyn Adapter>>;
}
