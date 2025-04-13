pub use tasks_impl::*;

#[cfg(not(target_arch = "wasm32"))]
mod tasks_impl {
    use std::future::Future;
    use bevy_tasks::Task;

    pub fn spawn_immediate_on_non_send<T>(func: impl FnOnce() + Send + 'static)
    where
        T: Send + 'static {
        bevy_tasks::ComputeTaskPool::get().spawn(async { func() }).detach();
    }

    pub fn spawn_compute<T>(future: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static {
        bevy_tasks::ComputeTaskPool::get().spawn(future)
    }

    pub fn spawn_async_compute<T>(future: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static {
        bevy_tasks::AsyncComputeTaskPool::get().spawn(future)
    }

    pub fn spawn_io<T>(future: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static {
        bevy_tasks::IoTaskPool::get().spawn(future)
    }
}

#[cfg(target_arch = "wasm32")]
mod tasks_impl {
    use std::future::Future;
    use bevy_tasks::Task;

    pub fn spawn_immediate_on_non_send<T>(func: impl FnOnce() + 'static)
    where
        T: 'static {
        func();
    }

    pub fn spawn_compute<T>(future: impl Future<Output = T> + 'static) -> Task<T>
    where
        T: 'static {
        bevy_tasks::ComputeTaskPool::get().spawn_local(future)
    }

    pub fn spawn_async_compute<T>(future: impl Future<Output = T> + 'static) -> Task<T>
    where
        T: 'static {
        bevy_tasks::AsyncComputeTaskPool::get().spawn_local(future)
    }

    pub fn spawn_io<T>(future: impl Future<Output = T> + 'static) -> Task<T>
    where
        T: 'static {
        bevy_tasks::IoTaskPool::get().spawn_local(future)
    }
}
