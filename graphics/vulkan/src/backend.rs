use sourcerenderer_core::gpu;

use super::*;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum VkBackend {}

impl gpu::GPUBackend for VkBackend {
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
    type MeshGraphicsPipeline = VkPipeline;
    type ComputePipeline = VkPipeline;
    type RayTracingPipeline = VkPipeline;
    type Swapchain = VkSwapchain;
    type TextureView = VkTextureView;
    type Sampler = VkSampler;
    type Fence = VkTimelineSemaphore;
    type Queue = VkQueue;
    type Heap = VkMemoryHeap;
    type QueryPool = VkQueryPool;
    type AccelerationStructure = VkAccelerationStructure;

    type SplitBarrier = VkEvent;

    fn name() -> &'static str {
        "Vulkan"
    }
}
