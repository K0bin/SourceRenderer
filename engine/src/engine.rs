use std::sync::{
    Arc,
    MutexGuard,
};

use log::trace;
use sourcerenderer_core::graphics::*;
use sourcerenderer_core::platform::{
    Event,
    Platform,
    Window,
};
use sourcerenderer_core::{
    Console,
    ThreadPoolBuilder,
};

use crate::asset::loaders::{
    FSContainer,
    ShaderLoader,
};
use crate::asset::AssetManager;
use crate::game::Game;
use crate::input::Input;
use crate::renderer::{
    LateLatchCamera,
    LateLatching,
    Renderer,
    RendererInterface,
};

const TICK_RATE: u32 = 5;

pub struct Engine<P: Platform> {
    renderer: Arc<Renderer<P>>,
    game: Arc<Game<P>>,
    asset_manager: Arc<AssetManager<P>>,
    input: Arc<Input>,
    late_latching: Option<Arc<dyn LateLatching<P::GraphicsBackend>>>,
    console: Arc<Console>,
}

impl<P: Platform> Engine<P> {
    #[cfg(not(feature = "web"))]
    pub fn initialize_global() {
        let cores = num_cpus::get();
        ThreadPoolBuilder::new()
            .num_threads(cores - 2)
            .build_global()
            .unwrap();
    }

    #[cfg(feature = "web")]
    pub fn initialize_global() {}

    pub fn run(platform: &P) -> Self {
        let instance = platform
            .create_graphics(false)
            .expect("Failed to initialize graphics");
        let surface = platform.window().create_surface(instance.clone());

        let console = Arc::new(Console::new());

        let input = Arc::new(Input::new());
        let mut adapters = instance.clone().list_adapters();
        let device = Arc::new(adapters.remove(0).create_device(&surface));
        let swapchain = Arc::new(platform.window().create_swapchain(true, &device, &surface));
        let asset_manager = AssetManager::<P>::new(platform, &device);
        asset_manager.add_container(Box::new(FSContainer::new(platform, &asset_manager)));
        asset_manager.add_loader(Box::new(ShaderLoader::new()));
        let late_latching = Arc::new(LateLatchCamera::new(
            device.as_ref(),
            swapchain.width() as f32 / swapchain.height() as f32,
            std::f32::consts::FRAC_PI_2,
        ));
        let late_latching_trait_obj =
            late_latching.clone() as Arc<dyn LateLatching<P::GraphicsBackend>>;
        let renderer = Renderer::<P>::run(
            platform,
            &instance,
            &device,
            &swapchain,
            &asset_manager,
            &input,
            Some(&late_latching_trait_obj),
            &console,
        );
        let game = Game::<P>::run(platform, &input, &renderer, &asset_manager, TICK_RATE);
        Self {
            renderer,
            game,
            asset_manager,
            input,
            late_latching: Some(late_latching),
            console,
        }
    }

    pub fn is_mouse_locked(&self) -> bool {
        self.input.poll().mouse_locked()
    }

    pub fn dispatch_event(&self, event: Event<P>) {
        match event {
            Event::MouseMoved(_)
            | Event::KeyUp(_)
            | Event::KeyDown(_)
            | Event::FingerDown(_)
            | Event::FingerUp(_)
            | Event::FingerMoved { .. } => {
                self.input.process_input_event(event);
            }
            Event::Quit => {
                self.stop();
            }
            Event::WindowMinimized
            | Event::WindowRestored(_)
            | Event::WindowSizeChanged(_)
            | Event::SurfaceChanged(_) => {
                let event_1 = event.clone();
                self.game.dispatch_window_event(event_1);
                self.renderer.dispatch_window_event(event);
            }
        }
    }

    pub fn instance(&self) -> &Arc<<P::GraphicsBackend as Backend>::Instance> {
        self.renderer.instance()
    }

    pub fn stop(&self) {
        trace!("Stopping engine");
        self.asset_manager.stop();
        self.renderer.unblock_game_thread();
        self.game.stop();
        self.renderer.stop();
    }

    pub fn is_running(&self) -> bool {
        if !self.game.is_running() || !self.renderer.is_running() {
            self.stop(); // if just one system dies, kill the others too
            return false;
        }
        true
    }

    pub fn device(&self) -> &Arc<<P::GraphicsBackend as Backend>::Device> {
        self.renderer.device()
    }

    pub fn surface(&self) -> MutexGuard<Arc<<P::GraphicsBackend as Backend>::Surface>> {
        self.renderer.surface()
    }

    pub fn frame(&self) {
        self.game.update(&self.renderer);
        self.renderer.render();
    }

    pub fn console(&self) -> &Console {
        self.console.as_ref()
    }
}
