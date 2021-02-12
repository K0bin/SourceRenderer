use std::sync::Arc;

use crate::graphics::{SampleCount, Format, Backend};

pub trait Surface {

}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainError {
  ZeroExtents,
  SurfaceLost,
  Other
}

pub trait Swapchain<B: Backend> : Sized {
  fn recreate(old: &Self, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError>;
  fn recreate_for_surface(old: &Self, surface: &Arc<B::Surface>, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError>;
  fn sample_count(&self) -> SampleCount;
  fn format(&self) -> Format;
  fn surface(&self) -> &Arc<B::Surface>;
}
