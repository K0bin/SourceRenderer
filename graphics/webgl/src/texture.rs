use std::sync::Arc;

use sourcerenderer_core::graphics::{AddressMode, Filter, Format, SamplerInfo, Texture, TextureDepthStencilView, TextureViewInfo, TextureInfo, TextureRenderTargetView, TextureSamplingView, TextureStorageView, CompareFunc};

use web_sys::{WebGl2RenderingContext, WebGlRenderingContext, WebglCompressedTextureS3tc};

use crate::{GLThreadSender, WebGLBackend, thread::{TextureHandle, SamplerHandle}};

pub struct WebGLTexture {
  handle: crate::thread::TextureHandle,
  sender: GLThreadSender,
  info: TextureInfo
}

unsafe impl Send for WebGLTexture {}
unsafe impl Sync for WebGLTexture {}

impl WebGLTexture {
  pub fn new(id: TextureHandle, info: &TextureInfo, sender: &GLThreadSender) -> Self {
    let c_info = info.clone();
    sender.send(Box::new(move |device| {
      device.create_texture(id, &c_info);
    }));

    Self {
      handle: id,
      sender: sender.clone(),
      info: info.clone()
    }
  }

  pub fn handle(&self) -> TextureHandle {
    self.handle
  }
}

impl Texture for WebGLTexture {
  fn info(&self) -> &TextureInfo {
    &self.info
  }
}

impl Drop for WebGLTexture {
  fn drop(&mut self) {
    let handle = self.handle;
    self.sender.send(Box::new(move |device| {
      device.remove_texture(handle);
    }));
  }
}

impl PartialEq for WebGLTexture {
  fn eq(&self, other: &Self) -> bool {
    self.handle() == other.handle()
  }
}

impl Eq for WebGLTexture {}

pub struct WebGLTextureSamplingView {
  texture: Arc<WebGLTexture>,
  info: TextureViewInfo
}

impl WebGLTextureSamplingView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  pub fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl TextureSamplingView<WebGLBackend> for WebGLTextureSamplingView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl PartialEq for WebGLTextureSamplingView {
  fn eq(&self, other: &Self) -> bool {
    self.texture == other.texture
  }
}

impl Eq for WebGLTextureSamplingView {}

pub struct WebGLRenderTargetView {
  texture: Arc<WebGLTexture>,
  info: TextureViewInfo
}

impl WebGLRenderTargetView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }
}

impl TextureRenderTargetView<WebGLBackend> for WebGLRenderTargetView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl PartialEq for WebGLRenderTargetView {
  fn eq(&self, other: &Self) -> bool {
    self.texture == other.texture
  }
}

impl Eq for WebGLRenderTargetView {}

pub struct WebGLDepthStencilView {
  texture: Arc<WebGLTexture>,
  info: TextureViewInfo
}

impl WebGLDepthStencilView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }

  pub fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  pub fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl TextureDepthStencilView<WebGLBackend> for WebGLDepthStencilView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl PartialEq for WebGLDepthStencilView {
  fn eq(&self, other: &Self) -> bool {
    self.texture == other.texture
  }
}

impl Eq for WebGLDepthStencilView {}

pub struct WebGLUnorderedAccessView {
  texture: Arc<WebGLTexture>,
  info: TextureViewInfo
}

impl WebGLUnorderedAccessView {
  pub fn new(texture: &Arc<WebGLTexture>, info: &TextureViewInfo) -> Self {
    Self {
      texture: texture.clone(),
      info: info.clone()
    }
  }
}

impl TextureStorageView<WebGLBackend> for WebGLUnorderedAccessView {
  fn texture(&self) -> &Arc<WebGLTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl PartialEq for WebGLUnorderedAccessView {
  fn eq(&self, other: &Self) -> bool {
    true
  }
}

impl Eq for WebGLUnorderedAccessView {}

pub struct WebGLSampler {
  handle: crate::thread::SamplerHandle,
  sender: GLThreadSender,
  info: SamplerInfo,
}

impl WebGLSampler {
  pub fn new(id: SamplerHandle, info: &SamplerInfo, sender: &GLThreadSender) -> Self {
    let c_info = info.clone();
    sender.send(Box::new(move |device| {
      device.create_sampler(id, &c_info);
    }));

    Self {
      handle: id,
      sender: sender.clone(),
      info: info.clone()
    }
  }

  pub fn handle(&self) -> TextureHandle {
    self.handle
  }
}

impl Drop for WebGLSampler {
  fn drop(&mut self) {
    let handle = self.handle;
    self.sender.send(Box::new(move |device| {
      device.remove_sampler(handle);
    }))
  }
}

pub(crate) fn format_to_type(_format: Format) -> u32 {
  WebGl2RenderingContext::UNSIGNED_BYTE
}

pub(crate) fn format_to_internal_gl(format: Format) -> u32 {
  match format {
    Format::D24 => WebGl2RenderingContext::DEPTH24_STENCIL8,
    Format::D32S8 => WebGl2RenderingContext::DEPTH32F_STENCIL8,
    Format::D32 => WebGl2RenderingContext::DEPTH_COMPONENT32F,
    Format::RGBA8UNorm => WebGl2RenderingContext::RGBA8,
    Format::DXT1 => WebglCompressedTextureS3tc::COMPRESSED_RGB_S3TC_DXT1_EXT,
    Format::DXT1Alpha => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT1_EXT,
    Format::DXT3 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT3_EXT,
    Format::DXT5 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT5_EXT,
    _ => panic!("Unsupported texture format {:?}", format)
  }
}

pub(crate) fn format_to_gl(format: Format) -> u32 {
  match format {
    Format::D24 => WebGl2RenderingContext::DEPTH_COMPONENT,
    Format::D32S8 => WebGl2RenderingContext::DEPTH_STENCIL,
    Format::D32 => WebGl2RenderingContext::DEPTH_COMPONENT,
    Format::RGBA8UNorm => WebGl2RenderingContext::RGBA,
    Format::DXT1 => WebglCompressedTextureS3tc::COMPRESSED_RGB_S3TC_DXT1_EXT,
    Format::DXT1Alpha => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT1_EXT,
    Format::DXT3 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT3_EXT,
    Format::DXT5 => WebglCompressedTextureS3tc::COMPRESSED_RGBA_S3TC_DXT5_EXT,
    _ => panic!("Unsupported texture format {:?}", format)
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

pub(crate) fn mag_filter_to_gl(filter: Filter) -> u32 {
  match filter {
    Filter::Linear => WebGlRenderingContext::LINEAR,
    Filter::Nearest => WebGlRenderingContext::NEAREST,
    _ => panic!("Unsupported filters: {:?}", filter)
  }
}

pub(crate) fn min_filter_to_gl(filter: Filter, mip_filter: Filter) -> u32 {
  match (filter, mip_filter) {
    (Filter::Linear, Filter::Linear) => WebGlRenderingContext::LINEAR_MIPMAP_LINEAR,
    (Filter::Linear, Filter::Nearest) => WebGlRenderingContext::LINEAR_MIPMAP_NEAREST,
    (Filter::Nearest, Filter::Linear) => WebGlRenderingContext::NEAREST_MIPMAP_LINEAR,
    (Filter::Nearest, Filter::Nearest) => WebGlRenderingContext::NEAREST_MIPMAP_NEAREST,
    _ => panic!("Unsupported filters: {:?}, {:?}", filter, mip_filter)
  }
}

pub(crate) fn compare_func_to_gl(compare_func: CompareFunc) -> u32 {
  match compare_func {
    CompareFunc::Never => WebGl2RenderingContext::NEVER,
    CompareFunc::Less => WebGl2RenderingContext::LESS,
    CompareFunc::LessEqual => WebGl2RenderingContext::LEQUAL,
    CompareFunc::Equal => WebGl2RenderingContext::EQUAL,
    CompareFunc::NotEqual => WebGl2RenderingContext::NOTEQUAL,
    CompareFunc::GreaterEqual => WebGl2RenderingContext::GEQUAL,
    CompareFunc::Greater => WebGl2RenderingContext::GREATER,
    CompareFunc::Always => WebGl2RenderingContext::ALWAYS,
  }
}
