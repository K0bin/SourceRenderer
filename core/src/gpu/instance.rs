use super::*;

pub trait Instance<B: GPUBackend> {
    fn list_adapters(&self) -> &[B::Adapter];
}
