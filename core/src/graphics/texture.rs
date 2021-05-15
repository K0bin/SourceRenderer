use crate::graphics::{Format, SampleCount, CompareFunc, Backend};
use std::sync::Arc;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct TextureInfo {
  pub format: Format,
  pub width: u32,
  pub height: u32,
  pub depth: u32,
  pub mip_levels: u32,
  pub array_length: u32,
  pub samples: SampleCount
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
