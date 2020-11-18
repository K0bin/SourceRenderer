use crate::graphics::{Format, SampleCount};

#[derive(Clone)]
pub enum SubpassOutput {
  Backbuffer {
    clear: bool
  },
  RenderTarget {
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth: u32,
    levels: u32,
    external: bool,
    load_action: LoadAction,
    store_action: StoreAction
  }
}

#[derive(Clone)]
pub struct DepthStencilOutput {
  pub name: String,
  pub format: Format,
  pub samples: SampleCount,
  pub extent: RenderPassTextureExtent,
  pub depth_load_action: LoadAction,
  pub depth_store_action: StoreAction,
  pub stencil_load_action: LoadAction,
  pub stencil_store_action: StoreAction
}

#[derive(Clone)]
pub enum ComputeOutput {
  RenderTarget {
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth: u32,
    levels: u32,
    external: bool,
    clear: bool
  },
  DepthStencil {
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    clear: bool
  },
  Backbuffer {
    clear: bool
   },
  Buffer {
    name: String,
    format: Option<Format>,
    size: u32,
    clear: bool
  }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExternalProducerType {
  Graphics,
  Compute,
  Copy,
  Host
}

#[derive(Clone)]
pub enum ExternalOutput {
  RenderTarget {
    name: String,
    producer_type: ExternalProducerType
  },
  DepthStencil {
    name: String,
    producer_type: ExternalProducerType
  },
  Buffer {
    name: String,
    producer_type: ExternalProducerType
  }
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
    outputs: Vec<ComputeOutput>
  },
  Transfer {
    inputs: Vec<PassInput>,
    outputs: Vec<ComputeOutput>
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
  pub external_resources: Vec<ExternalOutput>,
  pub swapchain_format: Format,
  pub swapchain_sample_count: SampleCount
}

pub trait RenderGraphTemplate {
}