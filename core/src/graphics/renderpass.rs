use std::sync::Arc;

use crate::graphics::Format;
use crate::graphics::SampleCount;
use crate::graphics::RenderTargetView;

#[derive(Clone, Copy, PartialEq)]
pub enum LoadOp {
  Load,
  Clear,
  DontCare
}

#[derive(Clone, Copy, PartialEq)]
pub enum StoreOp {
  Store,
  DontCare
}

#[derive(Clone, Copy, PartialEq)]
pub enum ImageLayout {
  Undefined,
  Common,
  RenderTarget,
  DepthWrite,
  DepthRead,
  ShaderResource,
  CopySrcOptimal,
  CopyDstOptimal,
  Present
}

#[derive(Clone, Copy, PartialEq)]
pub enum RenderpassRecordingMode {
  Commands,
  CommandBuffers
}

pub struct Attachment {
  pub format: Format,
  pub samples: SampleCount,
  pub load_op: LoadOp,
  pub store_op: StoreOp,
  pub stencil_load_op: LoadOp,
  pub stencil_store_op: StoreOp,
  pub initial_layout: ImageLayout,
  pub final_layout: ImageLayout
}

pub struct RenderPassLayoutInfo {
  pub attachments: Vec<Attachment>,
  pub subpasses: Vec<Subpass>
}

pub struct Subpass {
  pub input_attachments: Vec<AttachmentRef>,
  pub output_color_attachments: Vec<AttachmentRef>,
  pub output_resolve_attachments: Vec<AttachmentRef>,
  pub depth_stencil_attachment: Option<AttachmentRef>,
  pub preserve_unused_attachments: Vec<u32>
}

pub struct AttachmentRef {
  pub layout: ImageLayout,
  pub index: u32
}

pub trait RenderPassLayout {

}

pub struct RenderPassInfo {
  pub layout: Arc<dyn RenderPassLayout>,
  pub attachments: Vec<Arc<dyn RenderTargetView>>,
  pub width: u32,
  pub height: u32,
  pub array_length: u32
}

pub trait RenderPass {

}
