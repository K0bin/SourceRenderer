#![allow(dead_code)]

extern crate sourcerenderer_core;

extern crate ash;

extern crate vk_mem;
#[macro_use]
extern crate bitflags;
extern crate thread_local;
extern crate crossbeam_channel;
extern crate crossbeam_utils;
extern crate smallvec;
extern crate rayon;

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
pub use self::sync::VkSemaphore;
pub use self::sync::VkFence;
pub use self::pipeline::VkPipeline;
pub use self::renderpass::VkFrameBuffer;
pub use self::renderpass::VkRenderPass;
pub use self::backend::VkBackend;
pub(crate) use self::shared::VkShared;
pub(crate) use self::lifetime_tracker::VkLifetimeTrackers;
pub(crate) use self::thread_manager::VkThreadManager;
pub(crate) use self::sync::VkFenceInner;
pub(crate) use self::query::*;

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
mod thread_manager;
mod descriptor;
mod transfer;
mod shared;
mod lifetime_tracker;
mod query;
mod bindless;
mod rt;
