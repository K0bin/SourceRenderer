use crate::graphics::{Format, SampleCount, Backend, RenderGraphInfo};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub enum AttachmentInfo {
  Texture(TextureAttachmentInfo),
  Buffer
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum AttachmentSizeClass {
  Absolute,
  RelativeToSwapchain
}

#[derive(Clone)]
pub struct TextureAttachmentInfo {
  pub format: Format,
  pub samples: SampleCount,
  pub size_class: AttachmentSizeClass,
  pub width: f32,
  pub height: f32,
  pub levels: u32,
  pub external: bool
}

#[derive(Clone)]
pub enum PassInfo {
  Graphics(GraphicsPassInfo),
  Compute,
  Transfer,
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct OutputTextureAttachmentReference {
  pub name: String,
  pub load_action: LoadAction,
  pub store_action: StoreAction
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct InputTextureAttachmentReference {
  pub name: String,
  pub is_local: bool,
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

#[derive(Clone)]
pub struct GraphicsPassInfo {
  pub outputs: Vec<OutputTextureAttachmentReference>,
  pub inputs: Vec<InputAttachmentReference>
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