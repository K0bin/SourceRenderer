use sourcerenderer_core::gpu::*;

use super::*;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum VkBackend {}

impl GPUBackend for VkBackend {
    type Device = VkDevice;
    type Instance = VkInstance;
    type CommandBuffer = VkCommandBuffer;
    type CommandPool = VkCommandPool;
    type Adapter = VkAdapter;
    type Surface = VkSurface;
    type Texture = VkTexture;
    type Buffer = VkBuffer;
    type Shader = VkShader;
    type GraphicsPipeline = VkPipeline;
    type ComputePipeline = VkPipeline;
    type RayTracingPipeline = VkPipeline;
    type Swapchain = VkSwapchain;
    type TextureView = VkTextureView;
    type Sampler = VkSampler;
    type Fence = VkTimelineSemaphore;
    type Queue = VkQueue;
    type Heap = VkMemoryHeap;
    //type QueryRange = VkQueryRange;
    //type AccelerationStructure = VkAccelerationStructure;
}
