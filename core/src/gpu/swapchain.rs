use crate::Matrix4;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainError {
  ZeroExtents,
  SurfaceLost,
  Other
}

pub trait Swapchain<B: GPUBackend> : Sized {
  unsafe fn recreate(old: Self, width: u32, height: u32) -> Result<Self, SwapchainError>;
  unsafe fn recreate_on_surface(old: Self, surface: B::Surface, width: u32, height: u32) -> Result<Self, SwapchainError>;
  unsafe fn next_backbuffer(&self) -> Result<(), SwapchainError>;
  fn backbuffer(&self, index: u32) -> &B::Texture;
  fn backbuffer_index(&self) -> u32;
  fn backbuffer_count(&self) -> u32;
  fn sample_count(&self) -> SampleCount;
  fn format(&self) -> Format;
  fn surface(&self) -> &B::Surface;
  fn transform(&self) -> Matrix4;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
