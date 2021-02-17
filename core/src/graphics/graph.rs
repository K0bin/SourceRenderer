use std::collections::HashMap;
use std::sync::Arc;
use std::ops::Fn;

use crate::graphics::{Backend, SwapchainError};
use crate::graphics::command::InnerCommandBufferProvider;

pub enum RenderPassCallbacks<B: Backend> {
  Regular(Vec<Arc<dyn (Fn(&mut B::CommandBuffer, &dyn RenderGraphResources<B>)) + Send + Sync>>),
  InternallyThreaded(Vec<Arc<dyn (Fn(&Arc<dyn InnerCommandBufferProvider<B>>, &dyn RenderGraphResources<B>) -> Vec<B::CommandBufferSubmission>) + Send + Sync>>),
}

impl<B: Backend> Clone for RenderPassCallbacks<B> {
  fn clone(&self) -> Self {
    match self {
      Self::Regular(vec) => {
        Self::Regular(
          vec.iter().map(|c| c.clone()).collect()
        )
      }
      Self::InternallyThreaded(vec) => {
        Self::InternallyThreaded(
          vec.iter().map(|c| c.clone()).collect()
        )
      }
    }
  }
}

#[derive(Clone)]
pub struct RenderGraphInfo<B: Backend> {
  pub pass_callbacks: HashMap<String, RenderPassCallbacks<B>>
}

pub const BACK_BUFFER_ATTACHMENT_NAME: &str = "backbuffer";

pub enum ExternalResource<B: Backend> {
  Texture(Arc<B::TextureShaderResourceView>),
  Buffer(Arc<B::Buffer>)
}

impl<B: Backend> Clone for ExternalResource<B> {
  fn clone(&self) -> Self {
    match self {
      Self::Texture(view) => Self::Texture(view.clone()),
      Self::Buffer(buffer) => Self::Buffer(buffer.clone()),
    }
  }
}

pub trait RenderGraph<B: Backend> {
  fn recreate(old: &Self, swapchain: &Arc<B::Swapchain>) -> Self;
  fn render(&mut self) -> Result<(), SwapchainError>;
  fn swapchain(&self) -> &Arc<B::Swapchain>;
}

pub struct TextureDimensions {
  pub width: u32,
  pub height: u32,
  pub depth: u32,
  pub array_count: u32,
  pub mip_levels: u32
}

pub trait RenderGraphResources<B: Backend> : Send + Sync {
  fn get_buffer(&self, name: &str, history: bool) -> Result<&Arc<B::Buffer>, RenderGraphResourceError>;
  fn get_texture(&self, name: &str, history: bool) -> Result<&Arc<B::TextureShaderResourceView>, RenderGraphResourceError>;

  fn texture_dimensions(&self, name: &str) -> Result<TextureDimensions, RenderGraphResourceError>;
}

#[derive(Debug)]
pub enum RenderGraphResourceError {
  WrongResourceType,
  NotFound,
  NoHistory,
  NotAllowed
}
