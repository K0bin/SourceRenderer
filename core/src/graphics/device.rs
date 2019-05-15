use std::sync::Arc;

use graphics::Surface;

#[derive(Debug)]
pub enum AdapterType {
  DISCRETE,
  INTEGRATED,
  VIRTUAL,
  SOFTWARE,
  OTHER
}

pub trait Adapter {
  fn adapter_type(&self) -> AdapterType;
  fn create_device(self: Arc<Self>, surface: Arc<Surface>) -> Arc<dyn Device>;
}

pub trait Device {
  fn graphics_queue(&self) -> Arc<Queue>;
  fn presentation_queue(&self) -> Arc<Queue>;
  fn compute_queue(&self) -> Arc<Queue>;
  fn transfer_queue(&self) -> Arc<Queue>;
}

pub trait Queue {

}
