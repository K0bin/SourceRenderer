pub use self::device::Device;
pub use self::device::Adapter;
pub use self::device::AdapterType;
pub use self::instance::Instance;
pub use self::surface::Surface;
pub use self::surface::Swapchain;
pub use self::surface::SwapchainError;
pub use self::command::CommandBuffer;
pub use self::command::CommandBufferType;
pub use self::command::InnerCommandBufferProvider;
pub use self::buffer::Buffer;
pub use self::buffer::MappedBuffer;
pub use self::buffer::MutMappedBuffer;
pub use self::buffer::BufferUsage;
pub use self::buffer::BufferInfo;
pub use self::buffer::get_default_state;
pub use self::command::Queue;
pub use self::device::MemoryUsage;
pub use self::format::Format;
pub use self::pipeline::*;
pub use self::texture::Texture;
pub use self::texture::TextureInfo;
pub use self::texture::TextureUsage;
pub use self::renderpass::*;
pub use self::command::Viewport;
pub use self::command::Scissor;
pub use self::command::Barrier;
pub use self::backend::Backend;
pub use self::command::BindingFrequency;
pub use self::command::PipelineBinding;
pub use self::command::RenderPassBeginInfo;
pub use self::command::RenderPassAttachment;
pub use self::command::RenderPassAttachmentView;
pub use self::texture::{
  TextureShaderResourceView, TextureShaderResourceViewInfo, Filter, AddressMode, TextureUnorderedAccessView,
  SamplerInfo, TextureRenderTargetView, TextureRenderTargetViewInfo, TextureUnorderedAccessViewInfo,
  TextureDepthStencilView, TextureDepthStencilViewInfo 
};
pub use self::sync::Fence;
pub use self::graph_template::*;
pub use self::graph::*;

mod device;
mod instance;
mod surface;
mod command;
mod buffer;
mod format;
mod pipeline;
mod texture;
mod renderpass;
mod backend;
mod sync;
mod graph;
mod graph_template;
mod resource;

// TODO: find a better place for this
pub trait Resettable {
  fn reset(&mut self);
}
