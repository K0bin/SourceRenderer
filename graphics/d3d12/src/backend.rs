use sourcerenderer_core::gpu;

use super::*;

pub enum D3D12Backend {}

impl gpu::GPUBackend for D3D12Backend {
    type Instance;
    type Adapter;
    type Device;
    type Surface;
    type Swapchain;
    type CommandPool;
    type CommandBuffer;
    type Texture;
    type TextureView;
    type Sampler;
    type Buffer;
    type Shader;
    type GraphicsPipeline;
    type ComputePipeline;
    type RayTracingPipeline;
    type Fence;
    type Queue;
    type Heap;
    type AccelerationStructure;
}
