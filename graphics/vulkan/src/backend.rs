use crate::VkDevice;
use crate::*;
use crate::pipeline::VkShader;
use crate::graph::VkRenderGraph;
use buffer::VkBufferSlice;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum VkBackend {}

impl sourcerenderer_core::graphics::Backend for VkBackend {
  type Device = VkDevice;
  type CommandPool = VkCommandPool;
  type Instance = VkInstance;
  type CommandBuffer = VkCommandBufferRecorder;
  type CommandBufferSubmission = VkCommandBufferSubmission;
  type Adapter = VkAdapter;
  type Surface = VkSurface;
  type Texture = VkTexture;
  type Buffer = VkBufferSlice;
  type Shader = VkShader;
  type Pipeline = VkPipeline;
  type RenderTargetView = VkRenderTargetView;
  type Swapchain = VkSwapchain;
  type RenderGraph = VkRenderGraph;
}
