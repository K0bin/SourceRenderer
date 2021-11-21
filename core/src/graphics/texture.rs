use crate::graphics::{Format, SampleCount, CompareFunc, Backend};
use std::sync::Arc;

bitflags! {
  pub struct TextureUsage: u32 {
    const FRAGMENT_SHADER_SAMPLED            = 0b1;
    const VERTEX_SHADER_SAMPLED              = 0b10;
    const COMPUTE_SHADER_SAMPLED             = 0b100;
    const FRAGMENT_SHADER_STORAGE_READ       = 0b1000;
    const VERTEX_SHADER_STORAGE_READ         = 0b10000;
    const COMPUTE_SHADER_STORAGE_READ        = 0b100000;
    const FRAGMENT_SHADER_STORAGE_WRITE      = 0b1000000;
    const VERTEX_SHADER_STORAGE_WRITE        = 0b10000000;
    const COMPUTE_SHADER_STORAGE_WRITE       = 0b100000000;
    const FRAGMENT_SHADER_LOCAL              = 0b1000000000;
    const RENDER_TARGET                      = 0b10000000000;
    const DEPTH_READ                         = 0b100000000000;
    const DEPTH_WRITE                        = 0b1000000000000;
    const RESOLVE_SRC                        = 0b10000000000000;
    const RESOLVE_DST                        = 0b100000000000000;
    const BLIT_SRC                           = 0b1000000000000000;
    const BLIT_DST                           = 0b10000000000000000;
    const COPY_SRC                           = 0b100000000000000000;
    const COPY_DST                           = 0b1000000000000000000;
    const PRESENT                            = 0b10000000000000000000;

    const UNINITIALIZED                      = 0;
  }
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

