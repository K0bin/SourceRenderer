use crate::graphics::{Format, SampleCount, Backend, RenderGraphInfo};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub enum AttachmentInfo {
  Texture {
    format: Format,
    samples: SampleCount,
    size_class: AttachmentSizeClass,
    width: f32,
    height: f32,
    levels: u32,
    external: bool
  },
  Buffer
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum AttachmentSizeClass {
  Absolute,
  RelativeToSwapchain
}

#[derive(Clone)]
pub struct GraphicsSubpassInfo {
  pub outputs: Vec<OutputTextureAttachmentReference>,
  pub depth_stencil: Option<OutputTextureAttachmentReference>,
  pub inputs: Vec<InputAttachmentReference>
}

#[derive(Clone)]
pub struct PassInfo {
  pub name: String,
  pub pass_type: PassType
}

#[derive(Clone)]
pub enum PassType {
  Graphics {
    subpasses: Vec<GraphicsSubpassInfo>
  },
  Compute,
  Transfer,
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct OutputTextureAttachmentReference {
  pub name: String,
  pub load_action: LoadAction,
  pub store_action: StoreAction
}

#[derive(Clone)]
pub struct InputAttachmentReference {
  pub name: String,
  pub attachment_type: InputAttachmentReferenceType
}

#[derive(Clone)]
pub enum InputAttachmentReferenceType {
  Texture {
    is_local: bool,
  },
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

#[derive(Clone)]
pub struct RenderGraphTemplateInfo {
  pub attachments: HashMap<String, AttachmentInfo>,
  pub passes: Vec<PassInfo>,
  pub swapchain_format: Format,
  pub swapchain_sample_count: SampleCount
}

pub trait RenderGraphTemplate {
}