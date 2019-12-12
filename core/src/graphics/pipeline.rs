use std::sync::Arc;

use graphics::Format;
use graphics::RenderPassLayout;

#[derive(Clone, Copy, PartialEq)]
pub enum InputRate {
  PerVertex,
  PerInstance
}

pub struct ShaderInputElement {
  pub input_assembler_binding: u32,
  pub location_vk_mtl: u32,
  pub semantic_name_d3d: String,
  pub semantic_index_d3d: u32,
  pub offset: usize,
  pub format: Format
}

pub struct InputAssemblerElement {
  pub binding: u32,
  pub input_rate: InputRate,
  pub stride: usize
}

impl Default for ShaderInputElement {
  fn default() -> ShaderInputElement {
    return ShaderInputElement {
      input_assembler_binding: 0,
      location_vk_mtl: 0,
      semantic_name_d3d: String::new(),
      semantic_index_d3d: 0,
      offset: 0,
      format: Format::Unknown,
    };
  }
}

impl Default for InputAssemblerElement {
  fn default() -> InputAssemblerElement {
    return InputAssemblerElement {
      binding: 0,
      input_rate: InputRate::PerVertex,
      stride: 0
    };
  }
}

pub struct VertexLayoutInfo {
  pub shader_inputs: Vec<ShaderInputElement>,
  pub input_assembler: Vec<InputAssemblerElement>
}

// ignore input assembler for now and always use triangle lists

pub enum FillMode {
  Fill,
  Line
}

pub enum CullMode {
  None,
  Front,
  Back
}

pub enum FrontFace {
  CounterClockwise,
  Clockwise
}

#[derive(Clone, Copy, PartialEq)]
pub enum SampleCount {
  Samples1,
  Samples2,
  Samples4,
  Samples8
}

pub struct RasterizerInfo {
  pub fill_mode: FillMode,
  pub cull_mode: CullMode,
  pub front_face: FrontFace,
  pub sample_count: SampleCount
}

impl Default for RasterizerInfo {
  fn default() -> Self {
    return RasterizerInfo {
      fill_mode: FillMode::Fill,
      cull_mode: CullMode::Back,
      front_face: FrontFace::Clockwise,
      sample_count: SampleCount::Samples1
    };
  }
}

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Clone, Copy, PartialEq)]
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

pub struct StencilInfo {
  pub fail_op: StencilOp,
  pub depth_fail_op: StencilOp,
  pub pass_op: StencilOp,
  pub func: CompareFunc
}

impl Default for StencilInfo {
  fn default() -> Self {
    return StencilInfo {
        fail_op: StencilOp::Keep,
        depth_fail_op: StencilOp::Keep,
        pass_op: StencilOp::Keep,
        func: CompareFunc::Always
    };
  }
}

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
    return DepthStencilInfo {
      depth_test_enabled: true,
      depth_write_enabled: true,
      depth_func: CompareFunc::Less,
      stencil_enable: false,
      stencil_read_mask: 0,
      stencil_write_mask: 0,
      stencil_front: StencilInfo::default(),
      stencil_back: StencilInfo::default()
    };
  }
}

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Clone, Copy, PartialEq)]
pub enum BlendFactor {
  Zero,
  One,
  SrcColor,
  OneMinusSrcColor,
  DstColor,
  OneMinusDstColor,
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

#[derive(Clone, Copy, PartialEq)]
pub enum BlendOp {
  Add,
  Subtract,
  ReverseSubtract,
  Min,
  Max
}

pub struct BlendInfo {
  pub alpha_to_coverage_enabled: bool,
  pub logic_op_enabled: bool,
  pub logic_op: LogicOp,
  pub attachments: Vec<AttachmentBlendInfo>,
  pub constants: [f32; 4]
}

impl Default for BlendInfo {
  fn default() -> Self {
    return BlendInfo {
      alpha_to_coverage_enabled: false,
      logic_op_enabled: false,
      logic_op: LogicOp::And,
      attachments: Vec::new(),
      constants: [0f32, 0f32, 0f32, 0f32]
    };
  }
}

bitflags! {
  pub struct ColorComponents : u8 {
    const RED   = 0b0001;
    const GREEN = 0b0010;
    const BLUE  = 0b0100;
    const ALPHA = 0b1000;
  }
}

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
    return AttachmentBlendInfo {
      blend_enabled: false,
      src_color_blend_factor: BlendFactor::ConstantColor,
      dst_color_blend_factor: BlendFactor::ConstantColor,
      color_blend_op: BlendOp::Add,
      src_alpha_blend_factor: BlendFactor::ConstantColor,
      dst_alpha_blend_factor: BlendFactor::ConstantColor,
      alpha_blend_op: BlendOp::Add,
      write_mask: ColorComponents::RED | ColorComponents::GREEN | ColorComponents::BLUE | ColorComponents::ALPHA
    };
  }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ShaderType {
  VertexShader = 0,
  FragmentShader,
  GeometryShader,
  TessellationControlShader,
  TessellationEvaluationShader,
  ComputeShader,
  // TODO add RT shader types
  // TODO add mesh shaders (?)
}

pub trait Shader {
  fn get_shader_type(&self) -> ShaderType;
}

pub struct PipelineInfo {
  pub vs: Arc<dyn Shader>,
  pub fs: Option<Arc<dyn Shader>>,
  pub gs: Option<Arc<dyn Shader>>,
  pub tcs: Option<Arc<dyn Shader>>,
  pub tes: Option<Arc<dyn Shader>>,
  pub vertex_layout: VertexLayoutInfo,
  pub rasterizer: RasterizerInfo,
  pub depth_stencil: DepthStencilInfo,
  pub blend: BlendInfo,
  pub renderpass: Arc<dyn RenderPassLayout>,
  pub subpass: u32
  // TODO: pipeline layout
}

pub trait Pipeline {

}
