use crate::graphics::{Format, SampleCount, Backend, RenderGraphInfo};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub enum SubpassOutput {
  Backbuffer(BackbufferOutput),
  RenderTarget(RenderTargetOutput)
}

#[derive(Clone)]
pub struct BackbufferOutput {
  pub clear: bool
}

#[derive(Clone)]
pub struct RenderTargetOutput {
  pub name: String,
  pub format: Format,
  pub samples: SampleCount,
  pub extent: RenderPassTextureExtent,
  pub depth: u32,
  pub levels: u32,
  pub external: bool,
  pub load_action: LoadAction,
  pub store_action: StoreAction
}

#[derive(Clone)]
pub struct DepthStencilOutput {
  pub name: String,
  pub format: Format,
  pub samples: SampleCount,
  pub extent: RenderPassTextureExtent,
  pub load_action: LoadAction,
  pub store_action: StoreAction
}

#[derive(Clone)]
pub struct BufferOutput {
  pub name: String,
  pub format: Option<Format>,
  pub size: u32,
  pub clear: bool
}

#[derive(Clone)]
pub enum PassOutput {
  RenderTarget(RenderTargetOutput),
  DepthStencil(DepthStencilOutput),
  Backbuffer(BackbufferOutput),
  Buffer(BufferOutput)
}

#[derive(Clone)]
pub enum RenderPassTextureExtent {
  Absolute {
    width: u32,
    height: u32
  },
  RelativeToSwapchain {
    width: f32,
    height: f32
  }
}

#[derive(Clone)]
pub struct GraphicsSubpassInfo {
  pub outputs: Vec<SubpassOutput>,
  pub depth_stencil: Option<DepthStencilOutput>,
  pub inputs: Vec<PassInput>
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
  Compute {
    inputs: Vec<PassInput>,
    outputs: Vec<PassOutput>
  },
  Transfer {
    inputs: Vec<PassInput>,
    outputs: Vec<PassOutput>
  },
}

#[derive(Clone)]
pub struct PassInput {
  pub name: String,
  pub is_local: bool
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
  pub passes: Vec<PassInfo>,
  pub external_resources: Vec<PassOutput>,
  pub swapchain_format: Format,
  pub swapchain_sample_count: SampleCount
}

pub trait RenderGraphTemplate {
}