pub use device::*;
pub use context::*;
pub use texture::*;
pub use buffer::*;
pub use transfer::*;
pub use transient_buffer::*;
pub use allocator::*;
pub use memory::*;
use destroyer::*;
pub use command::*;
pub use sampler::*;
pub use queue::*;
pub use sync::*;
pub(super) use bindless::*;
pub use rt::*;
pub use swapchain::*;
pub use instance::*;
pub use pipeline::*;
pub use util::*;
pub use graphics_plugin::*;
pub use query::*;

pub use command::PipelineBinding; // why is this necessary?

mod device;
mod context;
mod texture;
mod buffer;
mod transient_buffer;
mod transfer;
mod allocator;
mod memory;
mod destroyer;
mod command;
mod sampler;
mod queue;
mod sync;
mod bindless;
mod rt;
mod pipeline;
mod swapchain;
mod instance;
mod util;
mod graphics_plugin;
mod query;

pub use sourcerenderer_core::gpu;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "dragonfly", target_os = "netbsd", target_os = "openbsd"))]
mod active_gpu_backend {
    pub use sourcerenderer_vulkan::VkBackend as Backend;
    pub use sourcerenderer_vulkan::VkInstance as Instance;
    pub use sourcerenderer_vulkan::VkAdapter as Adapter;
    pub use sourcerenderer_vulkan::VkDevice as Device;
    pub use sourcerenderer_vulkan::VkTexture as Texture;
    pub use sourcerenderer_vulkan::VkTextureView as TextureView;
    pub use sourcerenderer_vulkan::VkBuffer as Buffer;
    pub use sourcerenderer_vulkan::VkAccelerationStructure as AccelerationStructure;
    pub use sourcerenderer_vulkan::VkMemoryHeap as Heap;
    pub use sourcerenderer_vulkan::VkCommandPool as CommandPool;
    pub use sourcerenderer_vulkan::VkCommandBuffer as CommandBuffer;
    pub use sourcerenderer_vulkan::VkSampler as Sampler;
    pub use sourcerenderer_vulkan::VkSecondaryCommandBufferInheritance as CommandBufferInheritance;
    pub use sourcerenderer_vulkan::VkQueue as Queue;
    pub use sourcerenderer_vulkan::VkTimelineSemaphore as Fence;
    pub use sourcerenderer_vulkan::VkSwapchain as Swapchain;
    pub use sourcerenderer_vulkan::VkSurface as Surface;
    pub use sourcerenderer_vulkan::VkBackbufferIndices as Backbuffer;
    pub use sourcerenderer_vulkan::VkPipeline as GraphicsPipeline;
    pub use sourcerenderer_vulkan::VkPipeline as MeshGraphicsPipeline;
    pub use sourcerenderer_vulkan::VkPipeline as ComputePipeline;
    pub use sourcerenderer_vulkan::VkPipeline as RayTracingPipeline;
    pub use sourcerenderer_vulkan::VkQueryPool as QueryPool;
    pub use sourcerenderer_vulkan::VkShader as Shader;
    pub type Barrier<'a> = super::gpu::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> = super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
}

#[cfg(target_arch = "wasm32")]
mod active_gpu_backend {
    pub use sourcerenderer_webgpu::WebGPUBackend as Backend;
    pub use sourcerenderer_webgpu::WebGPUInstance as Instance;
    pub use sourcerenderer_webgpu::WebGPUAdapter as Adapter;
    pub use sourcerenderer_webgpu::WebGPUDevice as Device;
    pub use sourcerenderer_webgpu::WebGPUTexture as Texture;
    pub use sourcerenderer_webgpu::WebGPUTextureView as TextureView;
    pub use sourcerenderer_webgpu::WebGPUBuffer as Buffer;
    pub use sourcerenderer_webgpu::WebGPUAccelerationStructure as AccelerationStructure;
    pub use sourcerenderer_webgpu::WebGPUHeap as Heap;
    pub use sourcerenderer_webgpu::WebGPUCommandPool as CommandPool;
    pub use sourcerenderer_webgpu::WebGPUCommandBuffer as CommandBuffer;
    pub use sourcerenderer_webgpu::WebGPUSampler as Sampler;
    pub use sourcerenderer_webgpu::WebGPURenderBundleInheritance as CommandBufferInheritance;
    pub use sourcerenderer_webgpu::WebGPUQueue as Queue;
    pub use sourcerenderer_webgpu::WebGPUFence as Fence;
    pub use sourcerenderer_webgpu::WebGPUSwapchain as Swapchain;
    pub use sourcerenderer_webgpu::WebGPUSurface as Surface;
    pub use sourcerenderer_webgpu::WebGPUBackbuffer as Backbuffer;
    pub use sourcerenderer_webgpu::WebGPUGraphicsPipeline as GraphicsPipeline;
    pub use sourcerenderer_webgpu::WebGPUComputePipeline as ComputePipeline;
    pub use sourcerenderer_webgpu::WebGPUQueryPool as QueryPool;
    pub type RayTracingPipeline = <sourcerenderer_webgpu::WebGPUBackend as super::gpu::GPUBackend>::RayTracingPipeline;
    pub type MeshGraphicsPipeline = <sourcerenderer_webgpu::WebGPUBackend as super::gpu::GPUBackend>::MeshGraphicsPipeline;
    pub use sourcerenderer_webgpu::WebGPUShader as Shader;
    pub type Barrier<'a> = super::gpu::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> = super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod active_gpu_backend {
    pub use sourcerenderer_metal::MTLBackend as Backend;
    pub use sourcerenderer_metal::MTLInstance as Instance;
    pub use sourcerenderer_metal::MTLAdapter as Adapter;
    pub use sourcerenderer_metal::MTLDevice as Device;
    pub use sourcerenderer_metal::MTLTexture as Texture;
    pub use sourcerenderer_metal::MTLTextureView as TextureView;
    pub use sourcerenderer_metal::MTLBuffer as Buffer;
    pub use sourcerenderer_metal::MTLAccelerationStructure as AccelerationStructure;
    pub use sourcerenderer_metal::MTLHeap as Heap;
    pub use sourcerenderer_metal::MTLCommandPool as CommandPool;
    pub use sourcerenderer_metal::MTLCommandBuffer as CommandBuffer;
    pub use sourcerenderer_metal::MTLSampler as Sampler;
    pub type CommandBufferInheritance = std::sync::Arc<std::sync::Mutex<sourcerenderer_metal::MTLInnerCommandBufferInheritance>>;
    pub use sourcerenderer_metal::MTLQueue as Queue;
    pub use sourcerenderer_metal::MTLFence as Fence;
    pub use sourcerenderer_metal::MTLSwapchain as Swapchain;
    pub use sourcerenderer_metal::MTLSurface as Surface;
    pub use sourcerenderer_metal::MTLBackbuffer as Backbuffer;
    pub use sourcerenderer_metal::MTLGraphicsPipeline as GraphicsPipeline;
    pub use sourcerenderer_metal::MTLGraphicsPipeline as MeshGraphicsPipeline;
    pub use sourcerenderer_metal::MTLComputePipeline as ComputePipeline;
    pub use sourcerenderer_metal::MTLQueryPool as QueryPool;
    pub type RayTracingPipeline = ();
    pub use sourcerenderer_metal::MTLShader as Shader;
    pub type Barrier<'a> = super::gpu::singl::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> = super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
}

pub use active_gpu_backend::{
    Shader,
    GraphicsPipelineInfo,
    MeshGraphicsPipelineInfo,
    RayTracingPipelineInfo,
    Backend as ActiveBackend,
    Texture as BackendTexture,
    Instance as APIInstance,
    Backbuffer,
    Surface,
};

#[allow(unused)]
pub(crate) use self::gpu::{
    BINDLESS_TEXTURE_COUNT,
    LoadOpColor,
    LoadOpDepthStencil,
    BarrierSync,
    BarrierAccess,
    IndexFormat,
    ShaderType,
    Viewport,
    Scissor,
    Swapchain as CoreSwapchain,
    Device as CoreDevice,
    GPUBackend,
    QueueSharingMode,
    QueueType,
    TextureLayout,
    WHOLE_BUFFER,
    ShaderInputElement,
    LogicOp,
    AttachmentBlendInfo,
    BlendInfo,
    BlendFactor,
    BlendOp,
    InputAssemblerElement,
    VertexLayoutInfo,
    StencilInfo,
    RasterizerInfo,
    DepthStencilInfo,
    PrimitiveType,
    BarrierTextureRange,
    SwapchainError,
    InputRate,
    FillMode,
    CullMode,
    FrontFace,
    CompareFunc,
    RenderpassRecordingMode,
    ColorComponents,
    BindingType,
    OutOfMemoryError,
    QueueOwnershipTransfer,
    BindingInfo,
    ClearColor,
    ClearDepthStencilValue,
    PackedShader,
    ResolveMode,
    TextureSubresource,
    MemoryTextureCopyRegion,
    BufferTextureCopyRegion,
    BufferCopyRegion,
    DedicatedAllocationPreference,
};

pub use self::gpu::{
    BufferUsage,
    TextureUsage,
    BufferInfo,
    TextureInfo,
    TextureViewInfo,
    BindingFrequency,
    SampleCount,
    Format,
    TextureDimension,
    Filter,
    AddressMode,
    SamplerInfo,
    AdapterType,
};
