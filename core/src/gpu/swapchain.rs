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
  unsafe fn backbuffer(&self) -> &B::TextureView;
  fn sample_count(&self) -> SampleCount;
  fn format(&self) -> Format;
  fn surface(&self) -> &B::Surface;
  fn transform(&self) -> Matrix4;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
