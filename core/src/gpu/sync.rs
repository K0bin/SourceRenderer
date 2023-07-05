use super::*;

pub trait Fence {
  fn value(&self) -> u64;
  fn await_value(&self, value: u64);
}

pub struct FenceValuePair<B: GPUBackend> {
  pub fence: B::Fence,
  pub value: u64
}

pub struct FenceValuePairRef<'a, B: GPUBackend> {
  pub fence: &'a B::Fence,
  pub value: u64
}

pub enum FenceRef<'a, B: GPUBackend> {
  Fence(FenceValuePairRef<'a, B>),
  WSIFence(&'a B::WSIFence)
}

impl<B: GPUBackend> FenceValuePair<B> {
  pub fn is_signalled(&self) -> bool {
    self.fence.value() >= self.value
  }
}
