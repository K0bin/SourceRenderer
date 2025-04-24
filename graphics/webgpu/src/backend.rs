use sourcerenderer_core::gpu;

pub use crate::{
    adapter::*, buffer::*, command::*, device::*, instance::*, pipeline::*, query::*, queue::*,
    sampler::*, stubs::*, surface::*, swapchain::*, texture::*,
};

pub struct WebGPUBackend();

impl gpu::GPUBackend for WebGPUBackend {
    type Instance = WebGPUInstance;
    type Adapter = WebGPUAdapter;
    type Device = WebGPUDevice;
    type Surface = WebGPUSurface;
    type Swapchain = WebGPUSwapchain;
    type CommandPool = WebGPUCommandPool;
    type CommandBuffer = WebGPUCommandBuffer;
    type Texture = WebGPUTexture;
    type TextureView = WebGPUTextureView;
    type Sampler = WebGPUSampler;
    type Buffer = WebGPUBuffer;
    type Shader = WebGPUShader;
    type GraphicsPipeline = WebGPUGraphicsPipeline;
    type ComputePipeline = WebGPUComputePipeline;
    type RayTracingPipeline = ();
    type MeshGraphicsPipeline = ();
    type Fence = WebGPUFence;
    type Queue = WebGPUQueue;
    type Heap = WebGPUHeap;
    type AccelerationStructure = WebGPUAccelerationStructure;
    type QueryPool = WebGPUQueryPool;

    fn name() -> &'static str {
        "WebGPU"
    }
}
