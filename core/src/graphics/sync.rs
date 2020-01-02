pub trait Semaphore {

}

pub trait Fence {
  fn await(&mut self);
  fn is_signaled(&self) -> bool;
}
