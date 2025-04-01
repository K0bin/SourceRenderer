use std::sync::Arc;

use bevy_ecs::system::Resource;
use sourcerenderer_core::{platform::{GraphicsPlatform, Window, WindowProvider}, Platform};

use super::{active_gpu_backend, Device, Instance, Swapchain};

#[derive(Resource)]
pub struct GPUDeviceResource(pub Arc<Device>);

impl Drop for GPUDeviceResource {
    fn drop(&mut self) {
        log::warn!("Dropping GPUDevicePlugin");
    }
}

#[derive(Resource)]
pub struct GPUSwapchainResource(pub Swapchain);

pub(crate) fn initialize_graphics<P: Platform + GraphicsPlatform<active_gpu_backend::Backend> + WindowProvider<active_gpu_backend::Backend>>(platform: &P, app: &mut bevy_app::App) {
    let api_instance = platform
        .create_instance(true)
        .expect("Failed to initialize graphics");
    let gpu_instance = Instance::new(api_instance);

    let surface = platform.window().create_surface(gpu_instance.handle());

    let gpu_adapters = gpu_instance.list_adapters();
    let gpu_device = gpu_adapters.first().expect("No suitable GPU found").create_device(&surface);

    let core_swapchain = platform.window().create_swapchain(true, gpu_device.handle(), surface);
    let gpu_swapchain = Swapchain::new(core_swapchain, &gpu_device);

    let gpu_resource = GPUDeviceResource(gpu_device);
    let gpu_swapchain_resource = GPUSwapchainResource(gpu_swapchain);
    app.insert_resource(gpu_resource);
    app.insert_resource(gpu_swapchain_resource);
}
