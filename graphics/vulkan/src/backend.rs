use buffer::VkBufferSlice;
use texture::VkTextureView;

use crate::pipeline::VkShader;
use crate::rt::VkAccelerationStructure;
use crate::texture::VkSampler;
use crate::{
    VkDevice,
    *,
};

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
    type RayTracingPipeline = VkPipeline;
    type Swapchain = VkSwapchain;
    type TextureView = VkTextureView;
    type Sampler = VkSampler;
    type Fence = VkFence;
    type Semaphore = VkSemaphore;
    type Queue = VkQueue;
    type QueryRange = VkQueryRange;
    type AccelerationStructure = VkAccelerationStructure;
}
