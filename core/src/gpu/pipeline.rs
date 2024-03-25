use serde::Deserialize;
use serde::Serialize;

use super::*;

use std::hash::Hasher;
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum InputRate {
  PerVertex,
  PerInstance
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub struct ShaderInputElement {
  pub input_assembler_binding: u32,
  pub location_vk_mtl: u32,
  pub semantic_name_d3d: String,
  pub semantic_index_d3d: u32,
  pub offset: usize,
  pub format: Format
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub struct InputAssemblerElement {
  pub binding: u32,
  pub input_rate: InputRate,
  pub stride: usize
}

impl Default for ShaderInputElement {
  fn default() -> ShaderInputElement {
    ShaderInputElement {
      input_assembler_binding: 0,
      location_vk_mtl: 0,
      semantic_name_d3d: String::new(),
      semantic_index_d3d: 0,
      offset: 0,
      format: Format::Unknown,
    }
  }
}

impl Default for InputAssemblerElement {
  fn default() -> InputAssemblerElement {
    InputAssemblerElement {
      binding: 0,
      input_rate: InputRate::PerVertex,
      stride: 0
    }
  }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub struct VertexLayoutInfo<'a> {
  pub shader_inputs: &'a [ShaderInputElement],
  pub input_assembler: &'a [InputAssemblerElement]
}

// ignore input assembler for now and always use triangle lists
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillMode {
  Fill,
  Line
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CullMode {
  None,
  Front,
  Back
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontFace {
  CounterClockwise,
  Clockwise
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleCount {
  Samples1,
  Samples2,
  Samples4,
  Samples8
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub struct RasterizerInfo {
  pub fill_mode: FillMode,
  pub cull_mode: CullMode,
  pub front_face: FrontFace,
  pub sample_count: SampleCount
}

impl Default for RasterizerInfo {
  fn default() -> Self {
    RasterizerInfo {
      fill_mode: FillMode::Fill,
      cull_mode: CullMode::Back,
      front_face: FrontFace::Clockwise,
      sample_count: SampleCount::Samples1
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompareFunc {
  Never,
  Less,
  LessEqual,
  Equal,
  NotEqual,
  GreaterEqual,
  Greater,
  Always
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StencilOp {
  Keep,
  Zero,
  Replace,
  IncreaseClamp,
  DecreaseClamp,
  Invert,
  Increase,
  Decrease
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct StencilInfo {
  pub fail_op: StencilOp,
  pub depth_fail_op: StencilOp,
  pub pass_op: StencilOp,
  pub func: CompareFunc
}

impl Default for StencilInfo {
  fn default() -> Self {
    StencilInfo {
        fail_op: StencilOp::Keep,
        depth_fail_op: StencilOp::Keep,
        pass_op: StencilOp::Keep,
        func: CompareFunc::Always
    }
  }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct DepthStencilInfo {
  pub depth_test_enabled: bool,
  pub depth_write_enabled: bool,
  pub depth_func: CompareFunc,
  pub stencil_enable: bool,
  pub stencil_read_mask: u8,
  pub stencil_write_mask: u8,
  pub stencil_front: StencilInfo,
  pub stencil_back: StencilInfo
}

impl Default for DepthStencilInfo {
  fn default() -> Self {
    DepthStencilInfo {
      depth_test_enabled: true,
      depth_write_enabled: true,
      depth_func: CompareFunc::Less,
      stencil_enable: false,
      stencil_read_mask: 0,
      stencil_write_mask: 0,
      stencil_front: StencilInfo::default(),
      stencil_back: StencilInfo::default()
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicOp {
  Clear,
  Set,
  Copy,
  CopyInverted,
  Noop,
  Invert,
  And,
  Nand,
  Or,
  Nor,
  Xor,
  Equivalent,
  AndReversed,
  AndInverted,
  OrReverse,
  OrInverted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendFactor {
  Zero,
  One,
  SrcColor,
  OneMinusSrcColor,
  DstColor,
  OneMinusDstColor,
  SrcAlpha,
  OneMinusSrcAlpha,
  DstAlpha,
  OneMinusDstAlpha,
  ConstantColor,
  OneMinusConstantColor,
  SrcAlphaSaturate,
  Src1Color,
  OneMinusSrc1Color,
  Src1Alpha,
  OneMinusSrc1Alpha
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendOp {
  Add,
  Subtract,
  ReverseSubtract,
  Min,
  Max
}

#[derive(Debug, Clone)]
pub struct BlendInfo<'a> {
  pub alpha_to_coverage_enabled: bool,
  pub logic_op_enabled: bool,
  pub logic_op: LogicOp,
  pub attachments: &'a [AttachmentBlendInfo],
  pub constants: [f32; 4]
}

impl Hash for BlendInfo<'_> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.alpha_to_coverage_enabled.hash(state);
    self.logic_op_enabled.hash(state);
    self.logic_op.hash(state);
    self.attachments.hash(state);
    let constants_data: &[u32] = unsafe { std::slice::from_raw_parts(self.constants.as_ptr() as *const u32, self.constants.len()) };
    constants_data.hash(state);
  }
}

impl PartialEq for BlendInfo<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.alpha_to_coverage_enabled == other.alpha_to_coverage_enabled
    && self.logic_op_enabled == other.logic_op_enabled
    && self.logic_op == other.logic_op
    && self.attachments == other.attachments
    && (self.constants[0] - other.constants[0]).abs() < 0.01f32
    && (self.constants[1] - other.constants[1]).abs() < 0.01f32
    && (self.constants[2] - other.constants[2]).abs() < 0.01f32
    && (self.constants[3] - other.constants[3]).abs() < 0.01f32
  }
}

impl Eq for BlendInfo<'_> {}

impl Default for BlendInfo<'_> {
  fn default() -> Self {
    BlendInfo {
      alpha_to_coverage_enabled: false,
      logic_op_enabled: false,
      logic_op: LogicOp::And,
      attachments: &[],
      constants: [0f32, 0f32, 0f32, 0f32]
    }
  }
}

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub struct ColorComponents : u8 {
    const RED   = 0b0001;
    const GREEN = 0b0010;
    const BLUE  = 0b0100;
    const ALPHA = 0b1000;
  }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct AttachmentBlendInfo {
  pub blend_enabled: bool,
  pub src_color_blend_factor: BlendFactor,
  pub dst_color_blend_factor: BlendFactor,
  pub color_blend_op: BlendOp,
  pub src_alpha_blend_factor: BlendFactor,
  pub dst_alpha_blend_factor: BlendFactor,
  pub alpha_blend_op: BlendOp,
  pub write_mask: ColorComponents
}

impl Default for AttachmentBlendInfo {
  fn default() -> Self {
    AttachmentBlendInfo {
      blend_enabled: false,
      src_color_blend_factor: BlendFactor::ConstantColor,
      dst_color_blend_factor: BlendFactor::ConstantColor,
      color_blend_op: BlendOp::Add,
      src_alpha_blend_factor: BlendFactor::ConstantColor,
      dst_alpha_blend_factor: BlendFactor::ConstantColor,
      alpha_blend_op: BlendOp::Add,
      write_mask: ColorComponents::RED | ColorComponents::GREEN | ColorComponents::BLUE | ColorComponents::ALPHA
    }
  }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ShaderType {
  VertexShader = 0,
  FragmentShader,
  GeometryShader,
  TessellationControlShader,
  TessellationEvaluationShader,
  ComputeShader,
  RayGen,
  RayMiss,
  RayClosestHit,
  // TODO add mesh shaders (?)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PrimitiveType {
  Triangles,
  TriangleStrip,
  Lines,
  LineStrip,
  Points
}

pub trait Shader {
  fn shader_type(&self) -> ShaderType;
}

#[derive(Hash, Eq, PartialEq)]
pub struct GraphicsPipelineInfo<'a, B: GPUBackend> {
  pub vs: &'a B::Shader,
  pub fs: Option<&'a B::Shader>,
  pub vertex_layout: VertexLayoutInfo<'a>,
  pub rasterizer: RasterizerInfo,
  pub depth_stencil: DepthStencilInfo,
  pub blend: BlendInfo<'a>,
  pub primitive_type: PrimitiveType
}

impl<B: GPUBackend> Clone for GraphicsPipelineInfo<'_, B> {
  fn clone(&self) -> Self {
    Self {
      vs: self.vs,
      fs: self.fs,
      vertex_layout: self.vertex_layout.clone(),
      rasterizer: self.rasterizer.clone(),
      depth_stencil: self.depth_stencil.clone(),
      blend: self.blend.clone(),
      primitive_type: self.primitive_type
    }
  }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BindingType {
  StorageBuffer,
  StorageTexture,
  SampledTexture,
  ConstantBuffer,
  Sampler,
  TextureAndSampler
}

#[derive(Debug)]
pub struct BindingInfo<'a> {
  pub name: &'a str,
  pub binding_type: BindingType
}

pub trait ComputePipeline {
  fn binding_info(&self, set: BindingFrequency, slot: u32) -> Option<BindingInfo>;
}
