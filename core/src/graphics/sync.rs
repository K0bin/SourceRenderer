pub trait Fence {
  fn is_signaled(&self) -> bool;
  fn await_signal(&self);
}
