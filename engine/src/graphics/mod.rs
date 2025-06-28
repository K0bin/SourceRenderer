pub use allocator::*;
pub(super) use bindless::*;
pub use buffer::*;
pub use command::{
    PipelineBinding,
    *,
};
pub use context::*;
use destroyer::*;
pub use device::*;
pub use graphics_plugin::*;
pub use instance::*;
pub use memory::*;
pub use pipeline::*;
pub use query::*;
pub use queue::*;
pub use rt::*;
pub use sampler::*;
pub use swapchain::*;
pub use sync::*;
pub use texture::*;
pub use transfer::*;
pub use transient_buffer::*;
pub use util::*;
// why is this necessary?

mod allocator;
mod bindless;
mod buffer;
mod command;
mod context;
mod destroyer;
mod device;
mod graphics_plugin;
mod instance;
mod memory;
mod pipeline;
mod query;
mod queue;
mod rt;
mod sampler;
mod swapchain;
mod sync;
mod texture;
mod transfer;
mod transient_buffer;
mod util;

pub use sourcerenderer_core::gpu;

#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod active_gpu_backend {
    pub use sourcerenderer_vulkan::{
        VkAccelerationStructure as AccelerationStructure,
        VkAdapter as Adapter,
        VkBackbufferIndices as Backbuffer,
        VkBackend as Backend,
        VkBuffer as Buffer,
        VkCommandBuffer as CommandBuffer,
        VkCommandPool as CommandPool,
        VkDevice as Device,
        VkInstance as Instance,
        VkMemoryHeap as Heap,
        VkPipeline as GraphicsPipeline,
        VkPipeline as MeshGraphicsPipeline,
        VkPipeline as ComputePipeline,
        VkPipeline as RayTracingPipeline,
        VkQueryPool as QueryPool,
        VkQueue as Queue,
        VkSampler as Sampler,
        VkSecondaryCommandBufferInheritance as CommandBufferInheritance,
        VkShader as Shader,
        VkSurface as Surface,
        VkSwapchain as Swapchain,
        VkTexture as Texture,
        VkTextureView as TextureView,
        VkTimelineSemaphore as Fence,
        VkEvent as SplitBarrier,
    };
    pub type Barrier<'a> = super::gpu::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> =
        super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
}

#[cfg(target_arch = "wasm32")]
mod active_gpu_backend {
    pub use sourcerenderer_webgpu::{
        WebGPUAccelerationStructure as AccelerationStructure,
        WebGPUAdapter as Adapter,
        WebGPUBackbuffer as Backbuffer,
        WebGPUBackend as Backend,
        WebGPUBuffer as Buffer,
        WebGPUCommandBuffer as CommandBuffer,
        WebGPUCommandPool as CommandPool,
        WebGPUComputePipeline as ComputePipeline,
        WebGPUDevice as Device,
        WebGPUFence as Fence,
        WebGPUGraphicsPipeline as GraphicsPipeline,
        WebGPUHeap as Heap,
        WebGPUInstance as Instance,
        WebGPUQueryPool as QueryPool,
        WebGPUQueue as Queue,
        WebGPURenderBundleInheritance as CommandBufferInheritance,
        WebGPUSampler as Sampler,
        WebGPUSurface as Surface,
        WebGPUSwapchain as Swapchain,
        WebGPUTexture as Texture,
        WebGPUTextureView as TextureView,
    };
    pub type RayTracingPipeline =
        <sourcerenderer_webgpu::WebGPUBackend as super::gpu::GPUBackend>::RayTracingPipeline;
    pub type MeshGraphicsPipeline =
        <sourcerenderer_webgpu::WebGPUBackend as super::gpu::GPUBackend>::MeshGraphicsPipeline;
    pub use sourcerenderer_webgpu::WebGPUShader as Shader;
    pub type Barrier<'a> = super::gpu::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> =
        super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
    pub type SplitBarrier = <sourcerenderer_webgpu::WebGPUBackend as super::gpu::GPUBackend>::SplitBarrier;
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod active_gpu_backend {
    pub use sourcerenderer_metal::{
        MTLAccelerationStructure as AccelerationStructure,
        MTLAdapter as Adapter,
        MTLBackend as Backend,
        MTLBuffer as Buffer,
        MTLCommandBuffer as CommandBuffer,
        MTLCommandPool as CommandPool,
        MTLDevice as Device,
        MTLHeap as Heap,
        MTLInstance as Instance,
        MTLSampler as Sampler,
        MTLTexture as Texture,
        MTLTextureView as TextureView,
    };
    pub type CommandBufferInheritance =
        std::sync::Arc<std::sync::Mutex<sourcerenderer_metal::MTLInnerCommandBufferInheritance>>;
    pub use sourcerenderer_metal::{
        MTLBackbuffer as Backbuffer,
        MTLComputePipeline as ComputePipeline,
        MTLFence as Fence,
        MTLGraphicsPipeline as GraphicsPipeline,
        MTLGraphicsPipeline as MeshGraphicsPipeline,
        MTLQueryPool as QueryPool,
        MTLQueue as Queue,
        MTLSurface as Surface,
        MTLSwapchain as Swapchain,
    };
    pub type RayTracingPipeline = ();
    pub use sourcerenderer_metal::MTLShader as Shader;
    pub type Barrier<'a> = super::gpu::Barrier<'a, self::Backend>;
    pub type RenderTarget<'a> = super::gpu::RenderTarget<'a, self::Backend>;
    pub type AccelerationStructureInstance<'a> =
        super::gpu::AccelerationStructureInstance<'a, self::Backend>;
    pub type FenceValuePairRef<'a> = super::gpu::FenceValuePairRef<'a, self::Backend>;
    pub type Submission<'a> = super::gpu::Submission<'a, self::Backend>;
    pub type GraphicsPipelineInfo<'a> = super::gpu::GraphicsPipelineInfo<'a, self::Backend>;
    pub type MeshGraphicsPipelineInfo<'a> = super::gpu::MeshGraphicsPipelineInfo<'a, self::Backend>;
    pub type RayTracingPipelineInfo<'a> = super::gpu::RayTracingPipelineInfo<'a, self::Backend>;
    pub type SplitBarrier = <sourcerenderer_metal::MTLBackend as super::gpu::GPUBackend>::SplitBarrier;
}

pub use active_gpu_backend::{
    Backbuffer,
    Backend as ActiveBackend,
    GraphicsPipelineInfo,
    Instance as APIInstance,
    MeshGraphicsPipelineInfo,
    RayTracingPipelineInfo,
    Shader,
    Surface,
    Texture as BackendTexture,
};

pub use self::gpu::{
    AdapterType,
    AddressMode,
    BindingFrequency,
    BufferInfo,
    BufferUsage,
    Filter,
    Format,
    SampleCount,
    SamplerInfo,
    TextureDimension,
    TextureInfo,
    TextureUsage,
    TextureViewInfo,
};
#[allow(unused)]
pub(crate) use self::gpu::{
    AttachmentBlendInfo,
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BindingInfo,
    BindingType,
    BlendFactor,
    BlendInfo,
    BlendOp,
    BufferCopyRegion,
    BufferTextureCopyRegion,
    ClearColor,
    ClearDepthStencilValue,
    ColorComponents,
    CompareFunc,
    CullMode,
    DedicatedAllocationPreference,
    DepthStencilInfo,
    Device as CoreDevice,
    FillMode,
    FrontFace,
    GPUBackend,
    IndexFormat,
    InputAssemblerElement,
    InputRate,
    LoadOpColor,
    LoadOpDepthStencil,
    LogicOp,
    MemoryTextureCopyRegion,
    OutOfMemoryError,
    PackedShader,
    PrimitiveType,
    QueueOwnershipTransfer,
    QueueSharingMode,
    QueueType,
    RasterizerInfo,
    RenderpassRecordingMode,
    ResolveMode,
    Scissor,
    ShaderInputElement,
    ShaderType,
    StencilInfo,
    Swapchain as CoreSwapchain,
    SwapchainError,
    TextureLayout,
    TextureSubresource,
    VertexLayoutInfo,
    Viewport,
    BINDLESS_TEXTURE_COUNT,
    WHOLE_BUFFER,
};
