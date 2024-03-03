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

mod raw;

// pub use self::bindless::*;
//pub use self::query::*;
pub use self::queue::*;
pub use self::{
    adapter::*,
    backend::*,
    buffer::*,
    command::*,
    descriptor::*,
    device::*,
    format::*,
    instance::*,
    pipeline::*,
    renderpass::*,
    shared::*,
    surface::*,
    swapchain::*,
    sync::*,
    texture::*,
    heap::*,
    bindless::*,
    rt::*,
};
pub(crate) use crate::raw::*;

mod adapter;
mod backend;
mod bindless;
mod buffer;
mod command;
mod descriptor;
mod device;
mod format;
mod instance;
mod pipeline;
//mod query; // NEEDS REDESIGN
mod queue;
mod renderpass;
mod rt;
mod shared;
mod surface;
mod swapchain;
mod sync;
mod texture;
mod heap;
