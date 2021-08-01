use crate::graphics::{Instance, TextureShaderResourceView, Fence};
use crate::graphics::Adapter;
use crate::graphics::Device;
use crate::graphics::Surface;
use crate::graphics::CommandBuffer;
use crate::graphics::Shader;
use crate::graphics::Texture;
use crate::graphics::Buffer;
use crate::graphics::Swapchain;
use crate::graphics::{RenderGraph, RenderGraphTemplate};
use crate::graphics::TextureUnorderedAccessView;
use crate::graphics::TextureRenderTargetView;

use std::hash::Hash;

// WANT https://github.com/rust-lang/rust/issues/44265
pub trait Backend: 'static + Sized {
  type Instance: Instance<Self> + Send + Sync;
  type Adapter: Adapter<Self> + Send + Sync;
  type Device: Device<Self> + Send + Sync;
  type Surface: Surface + Send + Sync + PartialEq + Eq;
  type Swapchain: Swapchain<Self> + Send + Sync;
  type CommandBuffer: CommandBuffer<Self>;
  type CommandBufferSubmission: Send;
  type Texture: Texture + Send + Sync;
  type TextureShaderResourceView: TextureShaderResourceView<Self> + Send + Sync;
  type TextureUnorderedAccessView: TextureUnorderedAccessView<Self> + Send + Sync;
  type TextureRenderTargetView: TextureRenderTargetView<Self> + Send + Sync;
  type Sampler: Send + Sync;
  type Buffer: Buffer + Send + Sync;
  type Shader: Shader + Hash + Eq + PartialEq + Send + Sync;
  type GraphicsPipeline: Send + Sync;
  type ComputePipeline: Send + Sync;
  type RenderGraphTemplate: RenderGraphTemplate + Send + Sync;
  type RenderGraph: RenderGraph<Self> + Send + Sync;
  type Fence : Fence + Send + Sync;
}
