use std::sync::Arc;

use bevy_app::*;
use bevy_ecs::system::Resource;
use bevy_core::{FrameCountPlugin, TaskPoolPlugin};
use bevy_log::LogPlugin;
use bevy_time::{Fixed, Time, TimePlugin};
use bevy_transform::TransformPlugin;
use bevy_hierarchy::HierarchyPlugin;
use sourcerenderer_core::{gpu::GPUBackend, platform::Window, Console, Platform};

use crate::{asset::AssetManager, graphics::{Device, Instance, Swapchain}, renderer::RendererPlugin, transform::InterpolationPlugin};

#[derive(Resource)]
pub struct GPUDeviceResource<B: GPUBackend>(pub Arc<Device<B>>);

#[derive(Resource)]
pub struct GPUSwapchainResource<B: GPUBackend>(pub Swapchain<B>);

#[derive(Resource)]
pub struct AssetManagerResource<P: Platform>(pub Arc<AssetManager<P>>);

#[derive(Resource)]
pub struct ConsoleResource(pub Arc<Console>);

fn run<P: Platform>(platform: &P) {
    let api_instance = platform
            .create_graphics(true)
            .expect("Failed to initialize graphics");
    let gpu_instance = Instance::<P::GPUBackend>::new(api_instance);

    let surface = platform.window().create_surface(gpu_instance.handle());

    let console = Arc::new(Console::new());
    let console_resource = ConsoleResource(console);

    let gpu_adapters = gpu_instance.list_adapters();
    let gpu_device = gpu_adapters.first().expect("No suitable GPU found").create_device(&surface);


    let core_swapchain = platform.window().create_swapchain(true, gpu_device.handle(), surface);
    let gpu_swapchain = Swapchain::new(core_swapchain, &gpu_device);
    let asset_manager = AssetManager::<P>::new(platform, &gpu_device);
    let asset_manager_resource = AssetManagerResource(asset_manager);

    let gpu_resource = GPUDeviceResource::<P::GPUBackend>(gpu_device);
    let gpu_swapchain_resource = GPUSwapchainResource::<P::GPUBackend>(gpu_swapchain);

    App::new()
        .add_plugins(PanicHandlerPlugin::default())
        .add_plugins(LogPlugin::default())
        .add_plugins(TaskPoolPlugin::default())
        .add_plugins(TimePlugin::default())
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_plugins(FrameCountPlugin::default())
        .add_plugins(TransformPlugin::default())
        .add_plugins(HierarchyPlugin::default())
        .add_plugins(InterpolationPlugin::default())
        .insert_resource(console_resource)
        .insert_resource(gpu_resource)
        .insert_resource(gpu_swapchain_resource)
        .insert_resource(asset_manager_resource)
        .add_plugins(RendererPlugin::<P>::new())
        .run();
}