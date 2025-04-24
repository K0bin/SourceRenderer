mod raw;

pub use self::adapter::*;
pub use self::backend::*;
pub use self::bindless::*;
pub use self::buffer::*;
pub use self::command::*;
pub use self::descriptor::*;
pub use self::device::*;
pub use self::format::*;
pub use self::heap::*;
pub use self::instance::*;
pub use self::pipeline::*;
pub use self::query::*;
pub use self::queue::*;
pub(crate) use self::renderpass::*;
pub use self::rt::*;
pub use self::shared::*;
pub use self::surface::*;
pub use self::swapchain::*;
pub use self::sync::*;
pub use self::texture::*;
pub(crate) use crate::raw::*;

mod adapter;
mod backend;
mod bindless;
mod buffer;
mod command;
mod descriptor;
mod device;
mod format;
mod heap;
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

/*pub trait GraphicsPlatform : sourcerenderer_core::platform::GraphicsPlatform<VkBackend> {
    fn create_instance(&self, debug_layers: bool) -> Result<VkInstance, Box<dyn std::error::Error>>;
}

impl<T> GraphicsPlatform for T
    where T : sourcerenderer_core::platform::GraphicsPlatform<VkBackend> {
    fn create_instance(&self, debug_layers: bool) -> Result<VkInstance, Box<dyn std::error::Error>> {
        sourcerenderer_core::platform::GraphicsPlatform::<VkBackend>::create_instance(self, debug_layers)
    }
}*/
