use crate::Matrix4;

use super::*;

pub trait Surface<B: GPUBackend> {
  unsafe fn create_swapchain(self, width: u32, height: u32, vsync: bool, device: &B::Device) -> Result<B::Swapchain, SwapchainError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainError {
  Other,
  NeedsRecreation
}

pub trait Backbuffer {
  fn key(&self) -> u64;
}

pub trait Swapchain<B: GPUBackend> {
  #[cfg(not(feature = "non_send_gpu"))]
  type Backbuffer : Backbuffer;
  #[cfg(feature = "non_send_gpu")]
  type Backbuffer : Backbuffer;

  fn will_reuse_backbuffers(&self) -> bool;
  unsafe fn next_backbuffer(&mut self) -> Result<Self::Backbuffer, SwapchainError>;
  unsafe fn recreate(&mut self);
  unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a Self::Backbuffer) -> &'a B::Texture;
  fn format(&self) -> Format;
  fn surface(&self) -> &B::Surface;
  fn transform(&self) -> Matrix4;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
