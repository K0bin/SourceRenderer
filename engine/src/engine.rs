use std::marker::PhantomData;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bevy_input::keyboard::KeyboardInput;
use bevy_app::*;
use bevy_ecs::system::Resource;
use bevy_core::{FrameCountPlugin, TaskPoolPlugin};
use bevy_input::mouse::MouseMotion;
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_tasks::{ComputeTaskPool, IoTaskPool};
use bevy_time::{Fixed, Time, TimePlugin};
use bevy_transform::TransformPlugin;
use bevy_hierarchy::HierarchyPlugin;

use log::trace;
use sourcerenderer_core::platform::{
    Event,
    Platform,
    Window, IO,
};
use sourcerenderer_core::{
    Console,
    Vec2I, Vec2UI,
};

use crate::asset::loaders::{
    FSContainer, GltfLoader, ImageLoader, ShaderLoader
};
use crate::asset::{AssetContainer, AssetLoader, AssetManager, AssetManagerECSResource, AssetManagerPlugin};
use crate::graphics::*;
use crate::input::Input;
use crate::renderer::{Renderer, RendererPlugin};
use crate::transform::InterpolationPlugin;

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
        let console = Arc::new(Console::new());
        let console_resource = ConsoleResource(console);

        let mut app = App::new();
        initialize_graphics(platform, &mut app);

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
            .add_plugins(AssetManagerPlugin::<P>::default())
            .insert_resource(console_resource)
            .add_plugins(RendererPlugin::<P>::new())
            .add_plugins(game_plugins);

        if app.plugins_state() == PluginsState::Ready {
            app.finish();
            app.cleanup();
        }

        Self(app)
    }

    pub fn frame<P: Platform>(&mut self) {
        let app = &mut self.0;
        let plugins_state = app.plugins_state();
        if plugins_state == PluginsState::Ready {
            app.finish();
            app.cleanup();
            assert_eq!(app.plugins_state(), PluginsState::Cleaned);
        } else if plugins_state != PluginsState::Cleaned {
            #[cfg(not(target_arch = "wasm32"))] {
                // We only need to call it manually before the app is ready.
                // After that the TaskPoolPlugin takes care of it.
                bevy_tasks::tick_global_task_pools_on_main_thread();
                std::thread::sleep(Duration::from_millis(16u64));
            }

            return;
        }

        app.update();
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

    pub fn get_asset_manager<P: Platform>(app: &App) -> &Arc<AssetManager<P>> {
        &app.world().resource::<AssetManagerECSResource<P>>().0
    }
}
