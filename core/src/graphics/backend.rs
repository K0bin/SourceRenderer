use graphics::{Instance, TextureShaderResourceView, Fence};
use graphics::Adapter;
use graphics::Device;
use graphics::Surface;
use graphics::CommandBuffer;
use graphics::Shader;
use graphics::PipelineInfo;
use graphics::Texture;
use graphics::Buffer;
use graphics::Swapchain;
use graphics::Resettable;
use graphics::graph::RenderGraph;
use std::hash::Hash;

// WANT https://github.com/rust-lang/rust/issues/44265
pub trait Backend: 'static + Sized {
  type Instance: Instance<Self> + Send + Sync;
  type Adapter: Adapter<Self> + Send + Sync;
  type Device: Device<Self> + Send + Sync;
  type Surface: Surface + Send + Sync;
  type Swapchain: Swapchain + Send + Sync;
  type CommandBuffer: CommandBuffer<Self>;
  type Texture: Texture + Send + Sync;
  type TextureShaderResourceView: TextureShaderResourceView + Send + Sync;
  type Buffer: Buffer + Send + Sync;
  type Shader: Shader + Hash + Eq + PartialEq + Send + Sync;
  type RenderGraph: RenderGraph<Self> + Send + Sync;
  type Fence : Fence + Send + Sync;
}
