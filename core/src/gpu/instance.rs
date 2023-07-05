use super::*;

pub trait Instance<B: GPUBackend> {
  unsafe fn list_adapters(&self) -> Vec<B::Adapter>;
}
