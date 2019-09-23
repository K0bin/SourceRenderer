use std::rc::Rc;

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum CommandBufferType {
  PRIMARY,
  SECONDARY
}

pub trait CommandPool {
  fn create_command_buffer(self: Rc<Self>, command_buffer_type: CommandBufferType) -> Rc<CommandBuffer>;
  fn reset(&self);
}

pub trait CommandBuffer {
}
