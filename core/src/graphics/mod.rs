pub use self::device::Device;
pub use self::device::Adapter;
pub use self::device::AdapterType;
pub use self::device::Queue;
pub use self::device::QueueType;
pub use self::instance::Instance;
pub use self::surface::Surface;
pub use self::surface::Swapchain;
pub use self::surface::SwapchainInfo;
pub use self::command::CommandBuffer;
pub use self::command::CommandPool;
pub use self::command::CommandBufferType;
pub use self::buffer::Buffer;
pub use self::buffer::BufferUsage;
pub use self::device::MemoryUsage;
pub use self::format::Format;
pub use self::pipeline::*;
pub use self::texture::Texture;
pub use self::texture::RenderTargetView;
pub use self::sync::Fence;
pub use self::sync::Semaphore;
pub use self::renderpass::*;
pub use self::renderpass::RenderPassLayoutInfo;
pub use self::command::Viewport;
pub use self::command::Scissor;
pub use self::backend::Backend;

mod device;
mod instance;
mod surface;
mod command;
mod buffer;
mod format;
mod pipeline;
mod texture;
mod sync;
mod renderpass;
mod backend;

// TODO: find a better place for this
pub trait Resettable {
  fn reset(&mut self);
}
