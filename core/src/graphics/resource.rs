use super::BufferInfo;
use super::TextureInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonTextureUsage {
  Sample,
  StorageRead,
  ResolveSrc,
  BlitSrc,
  DepthRead
}

pub struct TextureResourceInfo {
  pub texture_info: TextureInfo,
  pub common_usage: CommonTextureUsage
}

pub trait TextureResource {}

pub struct BufferResourceInfo {
  pub buffer_info: BufferInfo
}

pub trait BufferResource {}
