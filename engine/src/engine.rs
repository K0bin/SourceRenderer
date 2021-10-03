use sourcerenderer_core::platform::Event;
use sourcerenderer_core::platform::Platform;
use std::sync::Arc;

use sourcerenderer_core::ThreadPoolBuilder;
use sourcerenderer_core::graphics::*;
use sourcerenderer_core::platform::Window;

use crate::{asset::AssetManager, renderer::RendererInterface};
use crate::renderer::Renderer;
use crate::game::Game;

const TICK_RATE: u32 = 5;

pub struct Engine<P: Platform> {
  renderer: Arc<Renderer<P>>,
  game: Arc<Game<P>>
}

impl<P: Platform> Engine<P> {
  pub fn initialize_global() {
    let cores = num_cpus::get();
    ThreadPoolBuilder::new().num_threads(cores - 2).build_global().unwrap();
  }

  pub fn run(platform: &P) -> Self {
    let instance = platform.create_graphics(false).expect("Failed to initialize graphics");
    let surface = platform.window().create_surface(instance.clone());

    let mut adapters = instance.clone().list_adapters();
    let device = Arc::new(adapters.remove(0).create_device(&surface));
    let swapchain = Arc::new(platform.window().create_swapchain(false, &device, &surface));
    let asset_manager = AssetManager::<P>::new(&device);
    let renderer = Renderer::<P>::run(platform.window(), &instance, &device, &swapchain, &asset_manager);
    let game = Game::<P>::run(&renderer, &asset_manager, TICK_RATE);
    Self {
      renderer,
      game
    }
  }

  pub fn is_mouse_locked(&self) -> bool {
    self.game.is_mouse_locked()
  }

  pub fn dispatch_event(&self, event: Event<P>) {
    match event {
      Event::MouseMoved(_)
      | Event::KeyUp(_)
      | Event::KeyDown(_)
      | Event::FingerDown(_)
      | Event::FingerUp(_)
      | Event::FingerMoved {..} => {
        self.game.process_input_event(event);
      },
      Event::Quit => {
        self.stop();
        return;
      },
      Event::WindowMinimized
      | Event::WindowRestored(_)
      | Event::WindowSizeChanged(_)
      | Event::SurfaceChanged(_) => {
        self.renderer.dispatch_window_event(event);
      }
    }
  }

  pub fn instance(&self) -> &Arc<<P::GraphicsBackend as Backend>::Instance> {
    self.renderer.instance()
  }

  pub fn stop(&self) {
    self.game.stop();
    self.renderer.stop();
  }

  pub fn is_running(&self) -> bool {
    if !self.game.is_running() || !self.renderer.is_running() {
      self.stop(); // if just one system dies, kill the others too
      return false;
    }
    return true;
  }
}
