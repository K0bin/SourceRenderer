use crate::VkDevice;
use crate::*;
use crate::pipeline::VkShader;
use crate::graph::VkRenderGraph;
use buffer::VkBufferSlice;
use texture::VkTextureShaderResourceView;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum VkBackend {}

impl sourcerenderer_core::graphics::Backend for VkBackend {
  type Device = VkDevice;
  type Instance = VkInstance;
  type CommandBuffer = VkCommandBufferRecorder;
  type Adapter = VkAdapter;
  type Surface = VkSurface;
  type Texture = VkTexture;
  type Buffer = VkBufferSlice;
  type Shader = VkShader;
  type Pipeline = VkPipeline;
  type Swapchain = VkSwapchain;
  type RenderGraph = VkRenderGraph;
  type TextureShaderResourceView = VkTextureShaderResourceView;
}
