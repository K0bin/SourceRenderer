use super::*;

bitflags! {
  #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
  pub struct TextureUsage: u32 {
    const SAMPLED       = 0b1;
    const RENDER_TARGET = 0b10;
    const STORAGE       = 0b100;
    const COPY_SRC      = 0b1000;
    const COPY_DST      = 0b10000;
    const RESOLVE_SRC   = 0b100000;
    const RESOLVE_DST   = 0b1000000;
    const BLIT_SRC      = 0b10000000;
    const BLIT_DST      = 0b100000000;
    const DEPTH_STENCIL = 0b1000000000;

    const GPU_WRITABLE = 0b10 | 0b1000000 | 0b100000000 | 0b1000000000;
  }
}

impl TextureUsage {
  pub fn gpu_writable(&self) -> bool {
    self.contains(Self::GPU_WRITABLE)
  }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum TextureLayout {
  Undefined,
  General,
  Sampled,
  Present,
  RenderTarget,
  DepthStencilRead,
  DepthStencilReadWrite,
  Storage,
  CopySrc,
  CopyDst,
  ResolveSrc,
  ResolveDst
}

impl Default for TextureLayout {
  fn default() -> Self {
    Self::Undefined
  }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum TextureDimension {
  Dim1D,
  Dim2D,
  Dim3D,
  Dim1DArray,
  Dim2DArray,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TextureInfo {
  pub dimension: TextureDimension,
  pub format: Format,
  pub width: u32,
  pub height: u32,
  pub depth: u32,
  pub mip_levels: u32,
  pub array_length: u32,
  pub samples: SampleCount,
  pub usage: TextureUsage,
  pub supports_srgb: bool
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Filter {
  Linear,
  Nearest,
  Min,
  Max,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum AddressMode {
  Repeat,
  MirroredRepeat,
  ClampToEdge,
  ClampToBorder
}


#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TextureViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_layer: u32,
  pub array_layer_length: u32,
  pub format: Option<Format>,
}

impl Default for TextureViewInfo {
  fn default() -> Self {
    Self {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_layer: 0,
      array_layer_length: 1,
      format: None,
    }
  }
}


#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TextureSubresource {
  pub array_layer: u32,
  pub mip_level: u32
}

#[derive(Clone)]
pub struct SamplerInfo {
  pub mag_filter: Filter,
  pub min_filter: Filter,
  pub mip_filter: Filter,
  pub address_mode_u: AddressMode,
  pub address_mode_v: AddressMode,
  pub address_mode_w: AddressMode,
  pub mip_bias: f32,
  pub max_anisotropy: f32,
  pub compare_op: Option<CompareFunc>,
  pub min_lod: f32,
  pub max_lod: Option<f32>,
}

pub trait Texture : Send + Sync + PartialEq + Eq {
  fn info(&self) -> &TextureInfo;
}

pub trait TextureView : Send + Sync + PartialEq + Eq {
  fn texture_info(&self) -> &TextureInfo;
  fn info(&self) -> &TextureViewInfo;
}

pub trait Sampler : Send + Sync {
  fn info(&self) -> &SamplerInfo;
}
