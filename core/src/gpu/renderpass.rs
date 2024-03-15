use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadOp {
  Load,
  Clear,
  DontCare
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderpassRecordingMode {
  Commands,
  CommandBuffers
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AttachmentInfo {
  pub format: Format,
  pub samples: SampleCount,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SubpassInfo<'a> {
  pub input_attachments: &'a [AttachmentRef],
  pub output_color_attachments: &'a [OutputAttachmentRef],
  pub depth_stencil_attachment: Option<DepthStencilAttachmentRef>
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DepthStencilAttachmentRef {
  pub index: u32,
  pub read_only: bool
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct OutputAttachmentRef {
  pub index: u32,
  pub resolve_attachment_index: Option<u32>
}


bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub struct RenderPassPipelineStage: u32 {
    const VERTEX   = 0b1;
    const FRAGMENT = 0b10;
    const BOTH     = 0b11;
  }
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AttachmentRef {
  pub index: u32,
  pub pipeline_stage: RenderPassPipelineStage
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RenderPassInfo<'a> {
  pub attachments: &'a [AttachmentInfo],
  pub subpasses: &'a [SubpassInfo<'a>]
}
