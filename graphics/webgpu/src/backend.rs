use sourcerenderer_core::gpu::GPUBackend;

pub use crate::{
    adapter::*,
    buffer::*,
    command::*,
    pipeline::*,
    queue::*,
    sampler::*,
    stubs::*,
    surface::*,
    swapchain::*,
    texture::*,
    device::*,
    instance::*,
};

pub struct WebGPUBackend();

impl GPUBackend for WebGPUBackend {
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
    type Fence = WebGPUFence;
    type Queue = WebGPUQueue;
    type Heap = WebGPUHeap;
    type AccelerationStructure = WebGPUAccelerationStructure;

    fn name() -> &'static str {
        "WebGPU"
    }
}
