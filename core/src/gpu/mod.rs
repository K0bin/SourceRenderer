pub use self::backend::*;
pub use self::buffer::*;
pub use self::command::*;
pub use self::descriptor_heap::*;
pub use self::device::*;
pub use self::format::*;
pub use self::heap::*;
pub use self::instance::*;
pub use self::pipeline::*;
pub use self::query::*;
pub use self::queue::*;
pub use self::rt::*;
pub use self::shader_metadata::*;
pub use self::swapchain::*;
pub use self::sync::*;
pub use self::texture::*;

mod backend;
mod buffer;
mod command;
mod descriptor_heap;
mod device;
mod format;
mod heap;
mod instance;
mod pipeline;
mod query;
mod queue;
mod rt;
mod shader_metadata;
mod swapchain;
mod sync;
mod texture;

// TODO: find a better place for this
pub trait Resettable {
    fn reset(&mut self);
}

#[cfg(feature = "non_send_gpu")]
mod send_sync_bounds {
    #[allow(unused)]
    pub trait GPUMaybeSend {}
    impl<T> GPUMaybeSend for T {}

    #[allow(unused)]
    pub trait GPUMaybeSync {}
    impl<T> GPUMaybeSync for T {}
}

#[cfg(not(feature = "non_send_gpu"))]
mod send_sync_bounds {
    #[allow(unused)]
    pub trait GPUMaybeSend: Send {}
    impl<T: Send> GPUMaybeSend for T {}

    #[allow(unused)]
    pub trait GPUMaybeSync: Sync {}
    impl<T: Sync> GPUMaybeSync for T {}
}

pub use send_sync_bounds::*;
