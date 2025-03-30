use sourcerenderer_core::gpu;

use super::*;

pub enum MTLBackend {}

pub(crate) type MTLRayTracingPipeline = (); 

impl gpu::GPUBackend for MTLBackend {
    type Instance = MTLInstance;
    type Adapter = MTLAdapter;
    type Device = MTLDevice;
    type Buffer = MTLBuffer;
    type Texture = MTLTexture;
    type Sampler = MTLSampler;
    type TextureView = MTLTextureView;
    type Queue = MTLQueue;
    type CommandPool = MTLCommandPool;
    type CommandBuffer = MTLCommandBuffer;
    type Surface = MTLSurface;
    type Swapchain = MTLSwapchain;
    type Fence = MTLFence;
    type Heap = MTLHeap;
    type Shader = MTLShader;
    type GraphicsPipeline = MTLGraphicsPipeline;
    type ComputePipeline = MTLComputePipeline;
    type QueryPool = MTLQueryPool;

    type RayTracingPipeline = MTLRayTracingPipeline;
    type AccelerationStructure = MTLAccelerationStructure;

    fn name() -> &'static str {
        "Metal"
    }
}
