use sourcerenderer_core::gpu;

use super::*;

pub enum D3D12Backend {}

impl gpu::GPUBackend for D3D12Backend {
    type Instance = D3D12Instance;
    type Adapter = D3D12Adapter;
    type Device = D3D12Device;
    type Surface;
    type Swapchain;
    type CommandPool;
    type CommandBuffer;
    type Texture;
    type TextureView;
    type Sampler;
    type Buffer = D3D12Buffer;
    type Shader;
    type GraphicsPipeline;
    type ComputePipeline;
    type RayTracingPipeline;
    type Fence;
    type Queue = D3D12Queue;
    type Heap = D3D12Heap;
    type AccelerationStructure;
}
