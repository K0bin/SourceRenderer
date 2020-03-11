pub trait Semaphore : Send + Sync {

}

pub trait Fence : Send + Sync {
  fn await(&mut self);
  fn is_signaled(&self) -> bool;
}
