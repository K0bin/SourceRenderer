use graphics::{Instance, TextureShaderResourceView};
use graphics::Adapter;
use graphics::Device;
use graphics::Surface;
use graphics::CommandBuffer;
use graphics::Shader;
use graphics::PipelineInfo;
use graphics::Pipeline;
use graphics::Texture;
use graphics::Buffer;
use graphics::Swapchain;
use graphics::Resettable;
use graphics::graph::RenderGraph;
use std::hash::Hash;

pub trait Backend: 'static + Sized {
  type Instance: Instance<Self>;
  type Adapter: Adapter<Self>;
  type Device: Device<Self>;
  type Surface: Surface;
  type Swapchain: Swapchain;
  type CommandBuffer: CommandBuffer<Self>;
  type Texture: Texture;
  type TextureShaderResourceView: TextureShaderResourceView;
  type Buffer: Buffer;
  type Shader: Shader + Hash + Eq + PartialEq;
  type Pipeline: Pipeline<Self>;
  type RenderGraph: RenderGraph<Self>;
}
