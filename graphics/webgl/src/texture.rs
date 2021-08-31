use std::{rc::Rc, sync::Arc};

use sourcerenderer_core::graphics::{AddressMode, Filter, Format, SamplerInfo, Texture, TextureDepthStencilView, TextureDepthStencilViewInfo, TextureInfo, TextureRenderTargetView, TextureRenderTargetViewInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, TextureUnorderedAccessView};

use web_sys::{WebGlRenderingContext, WebGlTexture as WebGLTextureHandle, WebglCompressedTextureS3tc};

use crate::{RawWebGLContext, WebGLBackend};

pub struct WebGLTexture {
  context: Rc<RawWebGLContext>,
  texture: WebGLTextureHandle,
  info: TextureInfo,
  is_cubemap: bool,
  target: u32,
}

unsafe impl Send for WebGLTexture {}
unsafe impl Sync for WebGLTexture {}

impl WebGLTexture {
  pub fn new(context: &Rc<RawWebGLContext>, info: &TextureInfo) -> Self {
    assert!(info.array_length == 6 || info.array_length == 1);
    let is_cubemap = info.array_length == 6;
    let target = if is_cubemap { WebGlRenderingContext::TEXTURE_BINDING_CUBE_MAP } else { WebGlRenderingContext::TEXTURE_BINDING_2D };
    let texture = context.create_texture().unwrap();
    Self {
      context: context.clone(),
      texture,
      info: info.clone(),
      is_cubemap,
      target
    }
  }

  pub fn handle(&self) -> &WebGLTextureHandle {
    &self.texture
  }

  pub fn is_cubemap(&self) -> bool {
    self.is_cubemap
  }

  pub fn target(&self) -> u32 {
    self.target
  }
}

impl Texture for WebGLTexture {
  fn get_info(&self) -> &TextureInfo {
    &self.info
  }
}

impl Drop for WebGLTexture {
  fn drop(&mut self) {
    self.context.delete_texture(Some(&self.texture));
  }
}

pub struct WebGLTextureShaderResourceView {
  texture: Arc<WebGLTexture>,
  info: TextureShaderResourceViewInfo
}

impl WebGLTextureShaderResourceView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureShaderResourceViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  pub fn info(&self) -> &TextureShaderResourceViewInfo {
    &self.info
  }
}

impl TextureShaderResourceView<WebGLBackend> for WebGLTextureShaderResourceView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture    
  }
}

pub struct WebGLRenderTargetView {
  texture: Arc<WebGLTexture>,
  info: TextureRenderTargetViewInfo
}

impl WebGLRenderTargetView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureRenderTargetViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  pub fn info(&self) -> &TextureRenderTargetViewInfo {
    &self.info
  }
}

impl TextureRenderTargetView<WebGLBackend> for WebGLRenderTargetView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }
}

pub struct WebGLDepthStencilView {
  texture: Arc<WebGLTexture>,
  info: TextureDepthStencilViewInfo
}

impl WebGLDepthStencilView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureDepthStencilViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  pub fn info(&self) -> &TextureDepthStencilViewInfo {
    &self.info
  }
}

impl TextureDepthStencilView<WebGLBackend> for WebGLDepthStencilView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }
}

pub struct WebGLUnorderedAccessView {}

impl TextureUnorderedAccessView<WebGLBackend> for WebGLUnorderedAccessView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    panic!("WebGL does not support storage textures")
  }
}

pub struct WebGLSampler {

}

impl WebGLSampler {
  pub fn new(info: &SamplerInfo) -> Self {
    Self {} 
  }
}

pub(crate) fn format_to_internal_gl(format: Format) -> u32 {
  match format {
    Format::RGBA8 => WebGlRenderingContext::RGBA,
    Format::DXT1 => WebglCompressedTextureS3tc::COMPRESSED_RGB_S3TC_DXT1_EXT,
    Format::DXT1Alpha => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT1_EXT,
    Format::DXT3 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT3_EXT,
    Format::DXT5 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT5_EXT,
    _ => panic!("Unsupported texture format")
  }
}

pub(crate) fn address_mode_to_gl(address_mode: AddressMode) -> u32 {
  match address_mode {
    AddressMode::ClampToBorder => WebGlRenderingContext::CLAMP_TO_EDGE,
    AddressMode::ClampToEdge => WebGlRenderingContext::CLAMP_TO_EDGE,
    AddressMode::Repeat => WebGlRenderingContext::REPEAT,
    AddressMode::MirroredRepeat => WebGlRenderingContext::MIRRORED_REPEAT
  }
}

pub(crate) fn max_filter_to_gl(filter: Filter) -> u32 {
  match filter {
    Filter::Linear => WebGlRenderingContext::LINEAR,
    Filter::Nearest => WebGlRenderingContext::NEAREST,
  }
}

pub(crate) fn min_filter_to_gl(filter: Filter, mip_filter: Filter) -> u32 {
  match (filter, mip_filter) {
    (Filter::Linear, Filter::Linear) => WebGlRenderingContext::LINEAR_MIPMAP_LINEAR,
    (Filter::Linear, Filter::Nearest) => WebGlRenderingContext::LINEAR_MIPMAP_NEAREST,
    (Filter::Nearest, Filter::Linear) => WebGlRenderingContext::NEAREST_MIPMAP_LINEAR,
    (Filter::Nearest, Filter::Nearest) => WebGlRenderingContext::NEAREST_MIPMAP_NEAREST,
  }
}
