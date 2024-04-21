use sourcerenderer_core::gpu;

use super::*;

pub enum D3D12Backend {}

impl gpu::GPUBackend for D3D12Backend {
    type Instance = D3D12Instance;
    type Adapter = D3D12Adapter;
    type Device = D3D12Device;
    type Surface = D3D12Surface;
    type Swapchain = D3D12Swapchain;
    type CommandPool = D3D12CommandPool;
    type CommandBuffer = D3D12CommandBuffer;
    type Texture = D3D12Texture;
    type TextureView = D3D12TextureView;
    type Sampler = D3D12Sampler;
    type Buffer = D3D12Buffer;
    type Shader = D3D12Shader;
    type GraphicsPipeline = D3D12Pipeline;
    type ComputePipeline = D3D12Pipeline;
    type RayTracingPipeline = D3D12Pipeline;
    type Fence = D3D12Fence;
    type Queue = D3D12Queue;
    type Heap = D3D12Heap;
    type AccelerationStructure = D3D12AccelerationStructure;
}
