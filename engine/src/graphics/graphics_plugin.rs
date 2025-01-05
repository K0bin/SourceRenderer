use std::sync::Arc;

use bevy_app::Plugin;
use bevy_ecs::system::Resource;
use sourcerenderer_core::{gpu::GPUBackend, platform::Window, Platform};

use super::{Device, Instance, Swapchain};

#[derive(Resource)]
pub struct GPUDeviceResource<B: GPUBackend>(pub Arc<Device<B>>);

#[derive(Resource)]
pub struct GPUSwapchainResource<B: GPUBackend>(pub Swapchain<B>);

pub(crate) fn initialize_graphics<P: Platform>(platform: &P, app: &mut bevy_app::App) {
    let api_instance = platform
        .create_graphics(true)
        .expect("Failed to initialize graphics");
    let gpu_instance = Instance::<P::GPUBackend>::new(api_instance);

    let surface = platform.window().create_surface(gpu_instance.handle());

    let gpu_adapters = gpu_instance.list_adapters();
    let gpu_device = gpu_adapters.first().expect("No suitable GPU found").create_device(&surface);

    let core_swapchain = platform.window().create_swapchain(true, gpu_device.handle(), surface);
    let gpu_swapchain = Swapchain::new(core_swapchain, &gpu_device);

    let gpu_resource = GPUDeviceResource::<P::GPUBackend>(gpu_device);
    let gpu_swapchain_resource = GPUSwapchainResource::<P::GPUBackend>(gpu_swapchain);
    app.insert_resource(gpu_resource);
    app.insert_resource(gpu_swapchain_resource);
}
