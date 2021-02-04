use std::collections::HashMap;
use std::sync::Arc;
use std::ops::Fn;

use crate::graphics::Backend;
use crate::graphics::command::InnerCommandBufferProvider;

pub type RegularRenderPassCallback<B: Backend> = dyn (Fn(&mut B::CommandBuffer, &dyn RenderGraphResources<B>)) + Send + Sync;
pub type InternallyThreadedRenderPassCallback<B: Backend> = dyn (Fn(&Arc<dyn InnerCommandBufferProvider<B>>, &dyn RenderGraphResources<B>) -> Vec<B::CommandBufferSubmission>) + Send + Sync;

#[derive(Clone)]
pub enum RenderPassCallbacks<B: Backend> {
  Regular(Vec<Arc<RegularRenderPassCallback<B>>>),
  InternallyThreaded(Vec<Arc<InternallyThreadedRenderPassCallback<B>>>),
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
  fn render(&mut self) -> Result<(), ()>;
}

pub trait RenderGraphResources<B: Backend> : Send + Sync {
  fn get_buffer(&self, name: &str, history: bool) -> Result<&Arc<B::Buffer>, RenderGraphResourceError>;
  fn get_texture(&self, name: &str, history: bool) -> Result<&Arc<B::TextureShaderResourceView>, RenderGraphResourceError>;
}

#[derive(Debug)]
pub enum RenderGraphResourceError {
  WrongResourceType,
  NotFound,
  NoHistory,
  NotAllowed
}
