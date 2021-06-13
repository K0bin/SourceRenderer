use crate::{graphics::{Format, SampleCount}, platform::Input};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PipelineStage {
  GraphicsShaders,
  VertexShader,
  FragmentShader,
  ComputeShader,
  Copy
}

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
pub enum DepthStencil {
  Output {
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth_load_action: LoadAction,
    depth_store_action: StoreAction,
    stencil_load_action: LoadAction,
    stencil_store_action: StoreAction
  },
  Input {
    name: String,
    is_history: bool
  },
  None
}

#[derive(Clone)]
pub enum Output {
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
  pub depth_stencil: DepthStencil,
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
    outputs: Vec<Output>
  },
  Copy {
    inputs: Vec<PassInput>,
    outputs: Vec<Output>
  },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum InputUsage {
  Storage,
  Sampled,
  Local,
  Copy
}

#[derive(Clone)]
pub struct PassInput {
  pub name: String,
  pub stage: PipelineStage,
  pub usage: InputUsage,
  pub is_history: bool
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
