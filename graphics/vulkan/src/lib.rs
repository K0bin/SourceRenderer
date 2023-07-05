#![allow(dead_code)]

extern crate sourcerenderer_core;

extern crate ash;

#[macro_use]
extern crate bitflags;
extern crate crossbeam_channel;
extern crate crossbeam_utils;
extern crate rayon;
extern crate smallvec;
extern crate thread_local;

pub use self::adapter::{
    VkAdapter,
    VkAdapterExtensionSupport,
};
pub use self::backend::VkBackend;
pub use self::buffer::VkBuffer;
pub use self::command::{
    VkCommandBufferRecorder,
    VkCommandBufferSubmission,
    VkCommandPool,
};
pub use self::device::VkDevice;
pub use self::instance::VkInstance;
pub(crate) use self::lifetime_tracker::VkLifetimeTrackers;
pub use self::pipeline::VkPipeline;
pub(crate) use self::query::*;
pub use self::queue::VkQueue;
pub use self::renderpass::{
    VkFrameBuffer,
    VkRenderPass,
};
pub(crate) use self::shared::VkShared;
pub use self::surface::VkSurface;
pub use self::swapchain::VkSwapchain;
pub use self::sync::{
    VkTimelineSemaphore,
};
pub use self::texture::VkTexture;
pub(crate) use self::thread_manager::VkThreadManager;

mod adapter;
mod backend;
mod bindless;
mod buffer;
mod command;
mod descriptor;
mod device;
mod format;
mod instance;
mod lifetime_tracker;
mod pipeline;
mod query;
mod queue;
mod raw;
mod renderpass;
mod rt;
mod shared;
mod surface;
mod swapchain;
mod sync;
mod texture;
mod thread_manager;
mod transfer;
