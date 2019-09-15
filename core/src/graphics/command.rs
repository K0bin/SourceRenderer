use std::rc::Rc;

pub trait CommandPool {
  fn create_command_buffer(self: Rc<Self>) -> Rc<CommandBuffer>;
  fn reset(&mut self);
}

pub trait CommandBuffer {

}
