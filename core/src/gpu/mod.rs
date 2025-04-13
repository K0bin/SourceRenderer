pub use self::device::*;
pub use self::instance::*;
pub use self::command::*;
pub use self::buffer::*;
pub use self::format::*;
pub use self::pipeline::*;
pub use self::texture::*;
pub use self::sync::*;
pub use self::swapchain::*;
pub use self::rt::*;
pub use self::descriptor_heap::*;
pub use self::queue::*;
pub use self::backend::*;
pub use self::heap::*;
pub use self::shader_metadata::*;
pub use self::query::*;

mod device;
mod instance;
mod swapchain;
mod command;
mod buffer;
mod format;
mod pipeline;
mod texture;
mod backend;
mod sync;
mod heap;
mod rt;
mod descriptor_heap;
mod queue;
mod shader_metadata;
mod query;

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
