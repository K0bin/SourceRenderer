use std::sync::Arc;

use super::*;

pub trait Fence {
    unsafe fn value(&self) -> u64;
    unsafe fn await_value(&self, value: u64);
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
    pub unsafe fn is_signalled(&self) -> bool {
        self.fence.value() >= self.value
    }

    pub unsafe fn await_signal(&self) {
        self.fence.await_value(self.value);
    }
}

pub struct SharedFenceValuePair<B: GPUBackend> {
  pub fence: Arc<B::Fence>,
  pub value: u64
}

impl<B: GPUBackend> Clone for SharedFenceValuePair<B> {
    fn clone(&self) -> Self {
        Self {
            fence: self.fence.clone(),
            value: self.value
        }
    }
}

impl<B: GPUBackend> SharedFenceValuePair<B> {
  pub unsafe fn is_signalled(&self) -> bool {
    self.fence.value() == self.value
  }

  pub unsafe fn await_signal(&self) {
    self.fence.await_value(self.value);
  }
}
