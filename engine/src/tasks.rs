pub use tasks_impl::*;

mod tasks_impl {
    use std::future::Future;

    use bevy_tasks::Task;
    use sourcerenderer_core::gpu::GPUMaybeSend;
    use sourcerenderer_core::platform::IOMaybeSend;

    pub fn spawn_compute<T>(future: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::ComputeTaskPool::get().spawn(future)
    }

    pub fn spawn_async_compute<T>(future: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::AsyncComputeTaskPool::get().spawn(future)
    }

    #[cfg(feature = "non_send_gpu")]
    pub fn spawn_compute_gpu_maybe_sendable<T>(
        future: impl Future<Output = T> + GPUMaybeSend + 'static,
    ) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::ComputeTaskPool::get().spawn_local(future)
    }

    #[cfg(feature = "non_send_gpu")]
    pub fn spawn_async_compute_gpu_maybe_sendable<T>(
        future: impl Future<Output = T> + GPUMaybeSend + 'static,
    ) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::AsyncComputeTaskPool::get().spawn_local(future)
    }

    #[cfg(not(feature = "non_send_gpu"))]
    pub fn spawn_compute_gpu_maybe_sendable<T>(
        future: impl Future<Output = T> + GPUMaybeSend + 'static,
    ) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::ComputeTaskPool::get().spawn(future)
    }

    #[cfg(not(feature = "non_send_gpu"))]
    pub fn spawn_async_compute_gpu_maybe_sendable<T>(
        future: impl Future<Output = T> + GPUMaybeSend + 'static,
    ) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::AsyncComputeTaskPool::get().spawn(future)
    }

    #[cfg(feature = "non_send_io")]
    pub fn spawn_io<T>(future: impl Future<Output = T> + IOMaybeSend + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::IoTaskPool::get().spawn_local(future)
    }

    #[cfg(not(feature = "non_send_io"))]
    pub fn spawn_io<T>(future: impl Future<Output = T> + IOMaybeSend + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        bevy_tasks::IoTaskPool::get().spawn(future)
    }
}
