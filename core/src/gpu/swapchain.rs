use std::hash::Hash;

use crate::Matrix4;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainError {
  Other,
  NeedsRecreation
}

pub trait Backbuffer {
  fn key(&self) -> u64;
}

pub trait Swapchain<B: GPUBackend> : Sized {
  type Backbuffer : Backbuffer + Send + Sync;

  unsafe fn next_backbuffer(&mut self) -> Result<Self::Backbuffer, SwapchainError>;
  unsafe fn recreate(&mut self);
  unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a Self::Backbuffer) -> &'a B::Texture;
  fn format(&self) -> Format;
  fn surface(&self) -> &B::Surface;
  fn transform(&self) -> Matrix4;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
