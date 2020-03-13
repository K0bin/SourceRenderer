extern crate sourcerenderer_core;

#[macro_use]
extern crate ash;

extern crate vk_mem;
#[macro_use]
extern crate bitflags;
extern crate thread_local;

pub use self::instance::VkInstance;
pub use self::adapter::VkAdapter;
pub use self::adapter::VkAdapterExtensionSupport;
pub use self::device::VkDevice;
pub use self::surface::VkSurface;
pub use self::swapchain::VkSwapchain;
pub use self::queue::VkQueue;
pub use self::command::VkCommandPool;
pub use self::command::VkCommandBufferSubmission;
pub use self::command::VkCommandBufferRecorder;
pub use self::buffer::VkBuffer;
pub use self::texture::VkTexture;
pub use self::texture::VkRenderTargetView;
pub use self::sync::VkSemaphore;
pub use self::sync::VkFence;
pub use self::pipeline::VkPipeline;
pub use self::renderpass::VkRenderPassLayout;
pub use self::renderpass::VkRenderPass;
pub use self::backend::VkBackend;

mod raw;
mod backend;
mod instance;
mod adapter;
mod device;
mod queue;
mod surface;
mod swapchain;
mod command;
mod buffer;
mod pipeline;
mod format;
mod texture;
mod sync;
mod renderpass;
mod graph;
mod context;
