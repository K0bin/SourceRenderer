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

#[allow(unused)]
pub(crate) use sourcerenderer_core::gpu::{
    BINDLESS_TEXTURE_COUNT,
    AdapterType,
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
    RayTracingPipelineInfo,
    GraphicsPipelineInfo,
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
    BufferCopyRegion
};



pub use sourcerenderer_core::gpu::{
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
};
