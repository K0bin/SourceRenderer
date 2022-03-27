use crate::graphics::{Instance, TextureSamplingView, Fence};
use crate::graphics::Adapter;
use crate::graphics::Device;
use crate::graphics::Surface;
use crate::graphics::CommandBuffer;
use crate::graphics::Shader;
use crate::graphics::Texture;
use crate::graphics::Buffer;
use crate::graphics::Swapchain;
use crate::graphics::TextureStorageView;
use crate::graphics::TextureRenderTargetView;
use super::{Queue, AccelerationStructure};
use super::texture::TextureDepthStencilView;

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
  type Texture: Texture + Send + Sync + PartialEq + Eq;
  type TextureSamplingView: TextureSamplingView<Self> + Send + Sync + PartialEq + Eq;
  type TextureStorageView: TextureStorageView<Self> + Send + Sync + PartialEq + Eq;
  type TextureRenderTargetView: TextureRenderTargetView<Self> + Send + Sync + PartialEq + Eq;
  type TextureDepthStencilView: TextureDepthStencilView<Self> + Send + Sync + PartialEq + Eq;
  type Sampler: Send + Sync;
  type Buffer: Buffer + Send + Sync;
  type Shader: Shader + Hash + Eq + PartialEq + Send + Sync;
  type GraphicsPipeline: Send + Sync;
  type ComputePipeline: Send + Sync;
  type RayTracingPipeline: Send + Sync;
  type Fence : Fence + Send + Sync;
  type Semaphore : Send + Sync;
  type Queue : Queue<Self> + Send + Sync;
  type QueryRange : Send + Sync;
  type AccelerationStructure : AccelerationStructure + Send + Sync;
}
