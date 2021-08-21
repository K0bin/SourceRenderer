use crate::{VkDevice, texture::VkSampler};
use crate::*;
use crate::pipeline::VkShader;
use buffer::VkBufferSlice;
use texture::VkTextureView;

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
  type ComputePipeline = VkPipeline;
  type Swapchain = VkSwapchain;
  type TextureShaderResourceView = VkTextureView;
  type TextureUnorderedAccessView = VkTextureView;
  type TextureRenderTargetView = VkTextureView;
  type TextureDepthStencilView = VkTextureView;
  type Sampler = VkSampler;
  type Fence = VkFence;
  type Semaphore = VkSemaphore;
  type Queue = VkQueue;
}
