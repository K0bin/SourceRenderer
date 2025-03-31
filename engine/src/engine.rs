use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

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

use log::{trace, warn};
use sourcerenderer_core::platform::{GraphicsPlatform, Platform, WindowProvider};
use sourcerenderer_core::{
    console::Console,
    Vec2UI,
};

use crate::asset::{AssetManager, AssetManagerECSResource, AssetManagerPlugin};
use crate::graphics::*;
use crate::renderer::RendererPlugin;
use crate::transform::InterpolationPlugin;

#[derive(Resource)]
pub struct ConsoleResource(pub Arc<Console>);

pub enum WindowState {
    Minimized,
    Window(Vec2UI),
    Fullscreen(Vec2UI)
}

pub const TICK_RATE: u32 = 5;


#[cfg(all(feature = "threading", target_arch = "wasm32"))]
compile_error!("Threads are not supported on WebAssembly.");

pub struct Engine{
    app: App,
    is_running: bool
}

impl Engine {
    pub fn run<P: Platform + GraphicsPlatform<ActiveBackend> + WindowProvider<ActiveBackend>, M>(platform: &P, game_plugins: impl Plugins<M>) -> Self {
        let console = Arc::new(Console::new());
        let console_resource = ConsoleResource(console);

        let mut app = App::new();
        initialize_graphics(platform, &mut app);

        app
            .add_plugins(PanicHandlerPlugin::default());

            #[cfg(not(target_arch = "wasm32"))]
            app.add_plugins(LogPlugin::default());

            app.add_plugins(TaskPoolPlugin::default())
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

        Self {
            app,
            is_running: true
        }
    }

    pub fn frame(&mut self) {
        if !self.is_running {
            warn!("Frame called after engine was stopped.");
            return;
        }

        let app = &mut self.app;
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
        if let Some(exit) = app.should_exit() {
            log::info!("Exiting because of app: {:?}", exit);
            self.is_running = false;
        }
    }

    pub fn is_mouse_locked(&self) -> bool {
        false
        //self.input.poll().mouse_locked()
    }

    pub fn dispatch_keyboard_input(&mut self, input: KeyboardInput) {
        self.app.world_mut().send_event(input);
    }

    pub fn dispatch_mouse_motion(&mut self, motion: MouseMotion) {
        self.app.world_mut().send_event(motion);
    }

    pub fn window_changed<P: Platform>(&mut self, window_state: WindowState) {
        RendererPlugin::<P>::window_changed(&self.app, window_state);
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn stop<P: Platform>(&mut self) {
        if !self.is_running {
            return;
        }
        self.is_running = false;
        trace!("Stopping engine");
        RendererPlugin::<P>::stop(&mut self.app);
    }

    pub fn debug_world(&self) {
        let entities = self.app.world().iter_entities();
        println!("WORLD");
        for entity in entities {
            let components = entity.archetype().components();
            for component in components {
                let component_name = self.app.world().components().get_name(component);
                println!("ENTITY: {:?}, COMPONENT: {:?}", entity.id(), component_name);
            }
        }
    }

    pub fn get_asset_manager<P: Platform>(app: &App) -> &Arc<AssetManager> {
        &app.world().resource::<AssetManagerECSResource<P>>().0
    }
}
