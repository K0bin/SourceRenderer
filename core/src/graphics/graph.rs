use std::collections::HashMap;
use std::sync::Arc;
use std::ops::Fn;

use crate::graphics::{Backend, SwapchainError};
use crate::graphics::command::InnerCommandBufferProvider;
use crate::Matrix4;

pub type RegularRenderPassCallback<B> = dyn (Fn(&mut <B as Backend>::CommandBuffer, &dyn RenderGraphResources<B>, u64)) + Send + Sync;
pub type InternallyThreadedRenderPassCallback<B> = dyn (Fn(&dyn InnerCommandBufferProvider<B>, &dyn RenderGraphResources<B>, u64) -> Vec<<B as Backend>::CommandBufferSubmission>) + Send + Sync;

pub enum RenderPassCallbacks<B: Backend> {
  Regular(Vec<Arc<RegularRenderPassCallback<B>>>),
  InternallyThreaded(Vec<Arc<InternallyThreadedRenderPassCallback<B>>>),
}

impl<B: Backend> Clone for RenderPassCallbacks<B> {
  fn clone(&self) -> Self {
    match self {
      Self::Regular(vec) => {
        Self::Regular(
          vec.to_vec()
        )
      }
      Self::InternallyThreaded(vec) => {
        Self::InternallyThreaded(
          vec.to_vec()
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
  fn get_texture_srv(&self, name: &str, history: bool) -> Result<&Arc<B::TextureShaderResourceView>, RenderGraphResourceError>;
  fn get_texture_uav(&self, name: &str, history: bool) -> Result<&Arc<B::TextureUnorderedAccessView>, RenderGraphResourceError>;

  fn texture_dimensions(&self, name: &str) -> Result<TextureDimensions, RenderGraphResourceError>;
  fn swapchain_transform(&self) -> &Matrix4;
}

#[derive(Debug)]
pub enum RenderGraphResourceError {
  WrongResourceType,
  NotFound,
  NoHistory,
  NotAllowed
}
