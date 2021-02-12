use sourcerenderer_core::platform::{Platform, PlatformEvent, WindowState, InputState, InputCommands};
use std::{time::SystemTime, sync::{Arc}};

use sourcerenderer_core::{Vec2, ThreadPoolBuilder};
use sourcerenderer_core::Vec3;
use sourcerenderer_core::graphics::*;
use sourcerenderer_core::platform::Window;

use crate::asset::{AssetManager, AssetType};
use crate::asset::loaders::CSGODirectoryContainer;
use crate::renderer::Renderer;
use crate::scene::Game;
use crate::fps_camera::{FPSCamera, fps_camera_rotation};

const TICK_RATE: u32 = 5;

pub struct Engine<P: Platform> {
  renderer: Arc<Renderer<P>>,
  game: Arc<Game<P>>,
  instance: Arc<<P::GraphicsBackend as Backend>::Instance>
}

impl<P: Platform> Engine<P> {
  pub fn run(platform: &P) -> Self {
    let cores = num_cpus::get();
    ThreadPoolBuilder::new().num_threads(cores - 2).build_global().unwrap();

    let instance = platform.create_graphics(true).expect("Failed to initialize graphics");
    let surface = platform.window().create_surface(instance.clone());

    let mut adapters = instance.clone().list_adapters();
    let device = Arc::new(adapters.remove(0).create_device(&surface));
    let swapchain = Arc::new(platform.window().create_swapchain(true, &device, &surface));
    let asset_manager = AssetManager::<P>::new(&device);
    let renderer = Renderer::<P>::run(platform.window(), &device, &swapchain, &asset_manager);
    let game = Game::<P>::run(&renderer, &asset_manager, TICK_RATE);
    Self {
      renderer,
      game,
      instance
    }
  }

  pub fn update_window_state(&self, state: WindowState) {
    self.renderer.set_window_state(state);
  }

  pub fn receive_input_commands(&self) -> InputCommands {
    self.game.receive_input_commands()
  }
  pub fn update_input_state(&self, input: InputState) {
    self.game.update_input_state(input);
  }
  pub fn replace_window(&self, window: Option<&P::Window>) {
    if let Some(window) = window {
      let surface = window.create_surface(self.instance.clone());
      self.renderer.change_surface(Some(&surface));
    } else {
      self.renderer.change_surface(None);
    }
  }
}
