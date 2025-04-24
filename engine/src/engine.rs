use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use bevy_app::*;
use bevy_diagnostic::FrameCountPlugin;
use bevy_ecs::resource::Resource;
use bevy_input::keyboard::KeyboardInput;
use bevy_input::mouse::MouseMotion;
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_time::{
    Fixed,
    Time,
    TimePlugin,
};
use bevy_transform::TransformPlugin;
use sourcerenderer_core::console::Console;
use sourcerenderer_core::platform::{
    GraphicsPlatform,
    PlatformIO,
    Window,
};
use sourcerenderer_core::Vec2UI;

use crate::asset::{
    AssetManager,
    AssetManagerECSResource,
    AssetManagerPlugin,
};
use crate::graphics::*;
use crate::renderer::RendererPlugin;
use crate::transform::InterpolationPlugin;

#[derive(Resource)]
pub struct ConsoleResource(pub Arc<Console>);

pub enum WindowState {
    Minimized,
    Window(Vec2UI),
    Fullscreen(Vec2UI),
}

pub const TICK_RATE: u32 = 5;

#[derive(PartialEq, Eq)]
pub enum EngineLoopFuncResult {
    KeepRunning,
    Exit,
}

#[cfg(all(feature = "threading", target_arch = "wasm32"))]
compile_error!("Threads are not supported on WebAssembly.");

pub struct Engine {
    app: App,
}

impl Drop for Engine {
    fn drop(&mut self) {
        log::info!("Stopping engine");
    }
}

impl Engine {
    pub fn run<M, IO: PlatformIO, G: GraphicsPlatform<ActiveBackend>>(
        window: &impl Window<ActiveBackend>,
        game_plugins: impl Plugins<M>,
    ) -> Self {
        let console = Arc::new(Console::new());
        let console_resource = ConsoleResource(console);

        let mut app = App::new();
        initialize_graphics::<G>(&mut app, window);

        app.add_plugins(PanicHandlerPlugin::default());

        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(LogPlugin::default());

        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(TerminalCtrlCHandlerPlugin::default());

        app.add_plugins(TaskPoolPlugin::default())
            .add_plugins(TimePlugin::default())
            .insert_resource(Time::<Fixed>::from_hz(TICK_RATE as f64))
            .add_plugins(FrameCountPlugin::default())
            .add_plugins(TransformPlugin::default())
            .add_plugins(InterpolationPlugin::default())
            .add_plugins(InputPlugin::default())
            .add_plugins(AssetManagerPlugin::<IO>::default())
            .insert_resource(console_resource)
            .add_plugins(RendererPlugin::<G>::new())
            .add_plugins(game_plugins);

        if app.plugins_state() == PluginsState::Ready {
            app.finish();
            app.cleanup();
        }

        Self { app }
    }

    pub fn frame(&mut self) -> EngineLoopFuncResult {
        crate::autoreleasepool(|| {
            let app = &mut self.app;
            let plugins_state = app.plugins_state();
            if plugins_state == PluginsState::Ready {
                app.finish();
                app.cleanup();
                assert_eq!(app.plugins_state(), PluginsState::Cleaned);
            } else if plugins_state != PluginsState::Cleaned {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // We only need to call it manually before the app is ready.
                    // After that the TaskPoolPlugin takes care of it.
                    bevy_tasks::tick_global_task_pools_on_main_thread();
                    std::thread::sleep(Duration::from_millis(16u64));
                }

                return EngineLoopFuncResult::KeepRunning;
            }

            if cfg!(not(target_arch = "wasm32")) {
                bevy_tasks::IoTaskPool::get().with_local_executor(|e| {
                    e.try_tick();
                });
                bevy_tasks::AsyncComputeTaskPool::get().with_local_executor(|e| {
                    e.try_tick();
                });
                bevy_tasks::ComputeTaskPool::get().with_local_executor(|e| {
                    e.try_tick();
                });
            }

            app.update();
            if let Some(exit) = app.should_exit() {
                log::info!("Exiting because of app: {:?}", exit);
                EngineLoopFuncResult::Exit
            } else {
                EngineLoopFuncResult::KeepRunning
            }
        })
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

    pub fn window_changed<P: GraphicsPlatform<ActiveBackend>>(
        &mut self,
        window_state: WindowState,
    ) {
        RendererPlugin::<P>::window_changed(&self.app, window_state);
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

    pub fn get_asset_manager(app: &App) -> &Arc<AssetManager> {
        &app.world().resource::<AssetManagerECSResource>().0
    }
}
