use std::hash::Hash;

use crate::Matrix4;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainError {
  NeedsRecreation
}

pub trait Backbuffer {
  fn key(&self) -> u64;
}

pub trait Swapchain<B: GPUBackend> : Sized {
  type Backbuffer : Backbuffer + PartialEq + Eq + Hash + Clone + Send + Sync;

  unsafe fn next_backbuffer(&mut self) -> Result<Self::Backbuffer, SwapchainError>;
  unsafe fn recreate(&mut self);
  unsafe fn texture_for_backbuffer(&self, backbuffer: &Self::Backbuffer) -> &B::Texture;
  fn format(&self) -> Format;
  fn surface(&self) -> &B::Surface;
  fn transform(&self) -> Matrix4;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
