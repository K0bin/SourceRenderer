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
  platform: Box<P>
}

impl<P: Platform> Engine<P> {
  pub fn run(platform: Box<P>) -> Self {
    let cores = num_cpus::get();
    ThreadPoolBuilder::new().num_threads(cores - 2).build_global().unwrap();

    let instance = platform.create_graphics(true).expect("Failed to initialize graphics");
    let surface = platform.window().create_surface(instance.clone());

    let mut adapters = instance.list_adapters();
    let device = Arc::new(adapters.remove(0).create_device(&surface));
    let swapchain = Arc::new(platform.window().create_swapchain(true, &device, &surface));
    let asset_manager = AssetManager::<P>::new(&device);
    let renderer = Renderer::<P>::run(platform.window(), &device, &swapchain, &asset_manager);
    let game = Game::<P>::run(&renderer, &asset_manager, TICK_RATE);
    Self {
      renderer,
      game,
      platform
    }
  }

  pub fn receive_input_commands(&self) -> InputCommands {
    self.game.receive_input_commands()
  }

  pub fn platform(&mut self) -> &mut P {
    &mut self.platform
  }
  pub fn poll_platform(&self) {
    self.game.update_input_state(self.platform.input_state());
    self.renderer.set_window_state(self.platform.window().state());
  }
}
