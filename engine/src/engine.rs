use std::marker::PhantomData;
use std::sync::Arc;

use bevy_input::keyboard::KeyboardInput;
use bevy_app::*;
use bevy_ecs::system::Resource;
use bevy_core::{FrameCountPlugin, TaskPoolPlugin};
use bevy_input::mouse::MouseMotion;
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_time::{Fixed, Time, TimePlugin};
use bevy_transform::TransformPlugin;
use bevy_hierarchy::HierarchyPlugin;

use log::trace;
use sourcerenderer_core::platform::{
    Event,
    Platform,
    Window,
};
use sourcerenderer_core::{
    Console,
    Vec2I, Vec2UI,
};

use crate::asset::loaders::{
    FSContainer, GltfLoader, ImageLoader, ShaderLoader
};
use crate::asset::{AssetContainer, AssetLoader, AssetManager};
use crate::graphics::*;
use crate::input::Input;
use crate::renderer::{Renderer, RendererPlugin};
use crate::transform::InterpolationPlugin;

#[derive(Resource)]
pub struct GPUDeviceResource<B: GPUBackend>(pub Arc<Device<B>>);

#[derive(Resource)]
pub struct GPUSwapchainResource<B: GPUBackend>(pub Swapchain<B>);

#[derive(Resource)]
pub struct AssetManagerResource<P: Platform>(pub Arc<AssetManager<P>>);

#[derive(Resource)]
pub struct ConsoleResource(pub Arc<Console>);

pub enum WindowState {
    Minimized,
    Window(Vec2UI),
    Fullscreen(Vec2UI)
}

pub const TICK_RATE: u32 = 5;

pub struct Engine(App);

impl Engine {
    pub fn run<P: Platform, M>(platform: &P, game_plugins: impl Plugins<M>) -> Self {
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

        let asset_manager: Arc<AssetManager<P>> = AssetManager::<P>::new(&gpu_device);
        asset_manager.add_container(FSContainer::new(platform, &asset_manager));
        asset_manager.add_loader(ShaderLoader::new());

        asset_manager.add_loader(GltfLoader::new());
        asset_manager.add_loader(ImageLoader::new());
        let asset_manager_resource = AssetManagerResource(asset_manager);

        let gpu_resource = GPUDeviceResource::<P::GPUBackend>(gpu_device);
        let gpu_swapchain_resource = GPUSwapchainResource::<P::GPUBackend>(gpu_swapchain);

        let mut app = App::new();

        app
            .add_plugins(PanicHandlerPlugin::default())
            .add_plugins(LogPlugin::default())
            .add_plugins(TaskPoolPlugin::default())
            .add_plugins(TimePlugin::default())
            .insert_resource(Time::<Fixed>::from_hz(TICK_RATE as f64))
            .add_plugins(FrameCountPlugin::default())
            .add_plugins(TransformPlugin::default())
            .add_plugins(HierarchyPlugin::default())
            .add_plugins(InterpolationPlugin::default())
            .add_plugins(InputPlugin::default())
            .insert_resource(console_resource)
            .insert_resource(gpu_resource)
            .insert_resource(gpu_swapchain_resource)
            .insert_resource(asset_manager_resource)
            .add_plugins(RendererPlugin::<P>::new())
            .add_plugins(game_plugins);

        if app.plugins_state() == PluginsState::Ready {
            app.finish();
            app.cleanup();
        }

        Self(app)
    }

    pub fn frame(&mut self) {
        self.0.update();
    }

    pub fn is_mouse_locked(&self) -> bool {
        false
        //self.input.poll().mouse_locked()
    }

    pub fn dispatch_keyboard_input(&mut self, input: KeyboardInput) {
        self.0.world_mut().send_event(input);
    }

    pub fn dispatch_mouse_motion(&mut self, motion: MouseMotion) {
        self.0.world_mut().send_event(motion);
    }

    pub fn window_changed<P: Platform>(&mut self, window_state: WindowState) {
        RendererPlugin::<P>::window_changed(&self.0, window_state);
    }

    pub fn is_running(&self) -> bool {
        !self.0.should_exit().is_some()
    }

    pub fn stop<P: Platform>(&self) {
        trace!("Stopping engine");
        self.0
            .world()
            .resource::<AssetManagerResource<P>>().0
            .stop();

        RendererPlugin::<P>::stop(&self.0);
    }

    pub fn debug_world(&self) {
        let entities = self.0.world().iter_entities();
        println!("WORLD");
        for entity in entities {
            let components = entity.archetype().components();
            for component in components {
                let component_name = self.0.world().components().get_name(component);
                println!("ENTITY: {:?}, COMPONENT: {:?}", entity.id(), component_name);
            }
        }
    }

    pub fn get_asset_manager<P: Platform>(app: &App) -> &AssetManager<P> {
        &app.world().resource::<AssetManagerResource<P>>().0
    }
}
