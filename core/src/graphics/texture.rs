use std::sync::Arc;

use graphics::{Backend, Format, SampleCount};

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

pub trait RenderTargetView<B: Backend> {
  fn get_texture(&self) -> Arc<B::Texture>;
}
