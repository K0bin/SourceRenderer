use crate::VkDevice;
use crate::*;
use crate::pipeline::VkShader;
use crate::graph::VkRenderGraph;
use buffer::VkBufferSlice;
use texture::VkTextureView;
use graph_template::VkRenderGraphTemplate;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum VkBackend {}

impl sourcerenderer_core::graphics::Backend for VkBackend {
  type Device = VkDevice;
  type Instance = VkInstance;
  type CommandBuffer = VkCommandBufferRecorder;
  type CommandBufferSubmission = VkCommandBufferSubmission;
  type Adapter = VkAdapter;
  type Surface = VkSurface;
  type Texture = VkTexture;
  type Buffer = VkBufferSlice;
  type Shader = VkShader;
  type GraphicsPipeline = VkPipeline;
  type Swapchain = VkSwapchain;
  type RenderGraph = VkRenderGraph;
  type RenderGraphTemplate = VkRenderGraphTemplate;
  type TextureShaderResourceView = VkTextureView;
  type Fence = VkFence;
}
