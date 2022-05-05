use crate::graphics::{Format, SampleCount, CompareFunc, Backend};
use std::sync::Arc;

bitflags! {
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
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TextureInfo {
  pub format: Format,
  pub width: u32,
  pub height: u32,
  pub depth: u32,
  pub mip_levels: u32,
  pub array_length: u32,
  pub samples: SampleCount,
  pub usage: TextureUsage
}

pub trait Texture {
  fn info(&self) -> &TextureInfo;
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum Filter {
  Linear,
  Nearest
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum AddressMode {
  Repeat,
  MirroredRepeat,
  ClampToEdge,
  ClampToBorder
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct TextureViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_level: u32,
  pub array_level_length: u32,
}

impl Default for TextureViewInfo {
  fn default() -> Self {
    Self {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1
    }
  }
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

pub trait TextureSamplingView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureStorageView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureRenderTargetView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureDepthStencilView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

