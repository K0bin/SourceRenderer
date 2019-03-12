extern crate sourcerenderer_core;

#[macro_use]
extern crate ash;

extern crate vk_mem;

pub use self::renderer::Renderer;
pub use self::instance::initialize_vulkan;

mod renderer;
mod resource;
mod presenter;
mod queue;
mod instance;
