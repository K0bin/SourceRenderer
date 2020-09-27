use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::cmp::Eq;
use std::ops::Fn;

use crate::graphics::{ Backend, VertexLayoutInfo, RasterizerInfo, DepthStencilInfo, BlendInfo, Format, SampleCount };
use crate::job::{JobQueue, JobCounterWait, JobScheduler};

#[derive(Clone)]
pub struct RenderGraphInfo<B: Backend> {
  pub attachments: HashMap<String, AttachmentInfo>,
  pub passes: Vec<PassInfo<B>>
}

pub struct GraphicsPassInfo<B: Backend> {
  pub outputs: Vec<OutputTextureAttachmentReference>,
  pub inputs: Vec<InputAttachmentReference>,
  pub render: Arc<dyn (Fn(&mut B::CommandBuffer) -> usize) + Send + Sync>
}

impl<B: Backend> Clone for GraphicsPassInfo<B> {
  fn clone(&self) -> Self {
    Self {
      outputs: self.outputs.clone(),
      inputs: self.inputs.clone(),
      render: self.render.clone()
    }
  }
}

#[derive(Clone)]
pub enum PassInfo<B: Backend> {
  Graphics(GraphicsPassInfo<B>),
  Compute,
  Transfer,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum AttachmentSizeClass {
  Absolute,
  RelativeToSwapchain
}

#[derive(Clone)]
pub enum AttachmentInfo {
  Texture(TextureAttachmentInfo),
  Buffer(BufferAttachmentInfo)
}

#[derive(Clone)]
pub struct TextureAttachmentInfo {
  pub format: Format,
  pub size_class: AttachmentSizeClass,
  pub width: f32,
  pub height: f32,
  pub levels: u32,
  pub samples: SampleCount,
  pub external: bool
}

#[derive(Clone)]
pub struct BufferAttachmentInfo {
  pub size: u32
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct InputTextureAttachmentReference {
  pub name: String,
  pub is_local: bool,
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct OutputTextureAttachmentReference {
  pub name: String,
  pub load_action: LoadAction,
  pub store_action: StoreAction
}

#[derive(Clone)]
pub enum InputAttachmentReference {
  Texture(InputTextureAttachmentReference),
  Buffer
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum StoreAction {
  Store,
  DontCare
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum LoadAction {
  Load,
  Clear,
  DontCare
}

pub const BACK_BUFFER_ATTACHMENT_NAME: &str = "backbuffer";

pub trait RenderGraph<B: Backend> {
  fn recreate(old: &Self, swapchain: &Arc<B::Swapchain>) -> Self;
  fn render(&mut self, job_queue: &dyn JobQueue) -> Result<JobCounterWait, ()>;
}

/*pub struct RenderGraphNode<'a> {
  pub pass: &'a RenderPassInfo<'a>,
  pub parent: &'a RenderGraphNode<'a>
}

struct RenderGraphTree<'a> {
  pub nodes: Vec<RenderGraphNode<'a>>,
  pub root: &'a RenderGraphNode<'a>
}

struct RenderGraphAttachment<'a> {
  output_by: &'a RenderPassInfo<'a>,
  input_for: HashSet<&'a RenderPassInfo<'a>>
}

fn analyze_render_graph<B: Backend>(info: &RenderGraphInfo) {
  let mut attachments: HashMap<&str, RenderGraphAttachment> = HashMap::new();
  for &pass in info.passes {
    for output in pass.outputs {
      if attachments.contains_key(&output.name as &str) {
        panic!("reused"); // TODO: handle errors gracefully
      }
      if pass.inputs.iter().any(|i| i.name == output.name) {
        panic!("hazard");
      }
      attachments.insert(&output.name, RenderGraphAttachment {
        output_by: pass,
        input_for: HashSet::new()
      });
    }
  }
  for &pass in info.passes {
    for input in pass.inputs {
      let attachment = attachments.get_mut(&input.name as &str).expect("undeclared input");
      attachment.input_for.insert(pass);
    }
  }
}*/
