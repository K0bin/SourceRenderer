use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::cmp::Eq;
use std::ops::Fn;

use crate::graphics::{ Backend, VertexLayoutInfo, RasterizerInfo, DepthStencilInfo, BlendInfo, Format, SampleCount };
use crate::job::JobScheduler;
use crate::graphics::command::InnerCommandBufferProvider;

pub type RegularRenderPassCallback<B: Backend> = dyn (Fn(&mut B::CommandBuffer)) + Send + Sync;
pub type InternallyThreadedRenderPassCallback<B: Backend> = dyn (Fn(&Arc<dyn InnerCommandBufferProvider<B>>) -> Vec<B::CommandBufferSubmission>) + Send + Sync;
pub type ThreadedRenderPassCallback<B: Backend> = dyn (Fn(&mut B::CommandBuffer)) + Send + Sync;

#[derive(Clone)]
pub enum RenderPassCallbacks<B: Backend> {
  Regular(Vec<Arc<RegularRenderPassCallback<B>>>),
  InternallyThreaded(Vec<Arc<InternallyThreadedRenderPassCallback<B>>>),
  Threaded(Vec<Arc<ThreadedRenderPassCallback<B>>>)
}

#[derive(Clone)]
pub struct RenderGraphInfo<B: Backend> {
  pub pass_callbacks: HashMap<String, RenderPassCallbacks<B>>
}

#[derive(Clone)]
pub struct BufferAttachmentInfo {
  pub size: u32
}

pub const BACK_BUFFER_ATTACHMENT_NAME: &str = "backbuffer";

pub trait RenderGraph<B: Backend> {
  fn recreate(old: &Self, swapchain: &Arc<B::Swapchain>) -> Self;
  fn render(&mut self) -> Result<(), ()>;
}
