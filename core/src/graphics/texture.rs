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
  fn get_info(&self) -> &TextureInfo;
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

#[derive(Clone)]
pub struct TextureShaderResourceViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_level: u32,
  pub array_level_length: u32,
}

#[derive(Clone)]
pub struct TextureRenderTargetViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_level: u32,
  pub array_level_length: u32,
}

#[derive(Clone)]
pub struct TextureUnorderedAccessViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_level: u32,
  pub array_level_length: u32,
}

#[derive(Clone)]
pub struct TextureDepthStencilViewInfo {
  pub base_mip_level: u32,
  pub mip_level_length: u32,
  pub base_array_level: u32,
  pub array_level_length: u32,
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
  pub max_lod: f32,
}

pub trait TextureShaderResourceView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureUnorderedAccessView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureRenderTargetView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

pub trait TextureDepthStencilView<B: Backend> {
  fn texture(&self) -> &Arc<B::Texture>;
}

