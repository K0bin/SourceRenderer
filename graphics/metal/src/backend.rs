use sourcerenderer_core::gpu;

use super::*;

pub enum MTLBackend {}

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
    
    type Shader;    
    type GraphicsPipeline;    
    type ComputePipeline;    
    type RayTracingPipeline;    
    type AccelerationStructure;
}
