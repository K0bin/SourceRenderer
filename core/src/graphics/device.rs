use std::sync::Arc;
use std::rc::Rc;

use graphics::Surface;
use graphics::CommandPool;

#[derive(Clone, Debug, Copy)]
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
  fn create_queue(self: Arc<Self>, queue_type: QueueType) -> Option<Arc<dyn Queue>>;
}

#[derive(Clone, Debug, Copy)]
pub enum QueueType {
  GRAPHICS,
  COMPUTE,
  TRANSFER
}

pub trait Queue {
  fn create_command_pool(self: Arc<Self>) -> Rc<CommandPool>;
  fn get_queue_type(&self) -> QueueType;
  fn supports_presentation(&self) -> bool;
}
