mod raw;

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
    shared::*,
    surface::*,
    swapchain::*,
    sync::*,
    texture::*,
    heap::*,
    bindless::*,
    rt::*,
    query::*
};
pub(crate) use self::renderpass::*;
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
mod query;
mod queue;
mod renderpass;
mod rt;
mod shared;
mod surface;
mod swapchain;
mod sync;
mod texture;
mod heap;

/*pub trait GraphicsPlatform : sourcerenderer_core::platform::GraphicsPlatform<VkBackend> {
    fn create_instance(&self, debug_layers: bool) -> Result<VkInstance, Box<dyn std::error::Error>>;
}

impl<T> GraphicsPlatform for T
    where T : sourcerenderer_core::platform::GraphicsPlatform<VkBackend> {
    fn create_instance(&self, debug_layers: bool) -> Result<VkInstance, Box<dyn std::error::Error>> {
        sourcerenderer_core::platform::GraphicsPlatform::<VkBackend>::create_instance(self, debug_layers)
    }
}*/
