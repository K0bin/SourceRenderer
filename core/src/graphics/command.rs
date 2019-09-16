use std::rc::Rc;

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum CommandBufferType {
  PRIMARY,
  SECONDARY
}

pub trait CommandPool {
  fn create_command_buffer(self: Rc<Self>, command_buffer_type: CommandBufferType) -> Rc<CommandBuffer>;
  fn reset(&mut self);
}

pub trait CommandBuffer {
  fn return_to_pool(self: Rc<Self>);
}
