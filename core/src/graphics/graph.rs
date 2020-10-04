use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::cmp::Eq;
use std::ops::Fn;

use crate::graphics::{ Backend, VertexLayoutInfo, RasterizerInfo, DepthStencilInfo, BlendInfo, Format, SampleCount };
use crate::job::{JobQueue, JobCounterWait, JobScheduler};

pub type RenderPassCallback<B: Backend> = dyn (Fn(&mut B::CommandBuffer) -> usize) + Send + Sync;

#[derive(Clone)]
pub struct RenderGraphInfo<B: Backend> {
  pub pass_callbacks: HashMap<String, Vec<Arc<RenderPassCallback<B>>>>
}

#[derive(Clone)]
pub struct BufferAttachmentInfo {
  pub size: u32
}

pub const BACK_BUFFER_ATTACHMENT_NAME: &str = "backbuffer";

pub trait RenderGraph<B: Backend> {
  fn recreate(old: &Self, swapchain: &Arc<B::Swapchain>) -> Self;
  fn render(&mut self, job_queue: &dyn JobQueue) -> Result<JobCounterWait, ()>;
}
