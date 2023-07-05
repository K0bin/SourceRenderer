use super::*;

pub trait DescriptorHeap<B: GPUBackend> {
  unsafe fn bind_sampling_view(&mut self, binding: u32, texture: &B::TextureView);
}
