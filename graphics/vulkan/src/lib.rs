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

pub use self::{
    adapter::{
        VkAdapter,
        VkAdapterExtensionSupport,
    },
    backend::VkBackend,
    buffer::VkBuffer,
    command::{
        VkCommandBufferRecorder,
        VkCommandBufferSubmission,
        VkCommandPool,
    },
    device::VkDevice,
    instance::VkInstance,
    pipeline::VkPipeline,
    queue::VkQueue,
    renderpass::{
        VkFrameBuffer,
        VkRenderPass,
    },
    surface::VkSurface,
    swapchain::VkSwapchain,
    sync::VkTimelineSemaphore,
    texture::VkTexture,
};
pub(crate) use self::{
    lifetime_tracker::VkLifetimeTrackers,
    query::*,
    shared::VkShared,
    thread_manager::VkThreadManager,
};

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

mod new;
