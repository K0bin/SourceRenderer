pub trait Fence {
  fn is_signaled(&self) -> bool;
  fn await(&self);
}
