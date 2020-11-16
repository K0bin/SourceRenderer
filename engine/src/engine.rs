use sourcerenderer_core::platform::{Platform, PlatformEvent};
use std::{time::SystemTime, sync::{Arc}};
use std::time::{Duration};

use sourcerenderer_core::{Vec2, ThreadPoolBuilder};
use sourcerenderer_core::Vec3;
use sourcerenderer_core::graphics::*;
use sourcerenderer_core::platform::Window;

use crate::asset::AssetManager;
use crate::renderer::Renderer;
use crate::scene::Scene;
use crate::fps_camera::{FPSCamera, fps_camera_rotation};

const TICK_RATE: u32 = 5;

pub struct Engine<P: Platform> {
    platform: Box<P>
}

struct Vertex {
  pub position: Vec3,
  pub color: Vec3,
  pub uv: Vec2
}

impl<P: Platform> Engine<P> {
  pub fn new(platform: Box<P>) -> Box<Engine<P>> {
    return Box::new(Engine {
      platform
    });
  }

  pub fn run(&mut self) {
    let cores = num_cpus::get();
    ThreadPoolBuilder::new().num_threads(cores - 2).build_global().unwrap();

    let instance = self.platform.create_graphics(true).expect("Failed to initialize graphics");
    let surface = self.platform.window().create_surface(instance.clone());

    let mut adapters = instance.list_adapters();
    let device = Arc::new(adapters.remove(0).create_device(&surface));
    let swapchain = Arc::new(self.platform.window().create_swapchain(true, &device, &surface));
    let asset_manager = AssetManager::<P>::new(&device);
    let renderer = Renderer::<P>::run(self.platform.window(), &device, &swapchain, &asset_manager, TICK_RATE);
    Scene::run::<P>(&renderer, &asset_manager, self.platform.input(), TICK_RATE);

    let mut fps_camera = FPSCamera::new();
    let mut last_iter_time = SystemTime::now();
    let event_tick_rate = 256;
    'event_loop: loop {
      let now = SystemTime::now();
      let delta = now.duration_since(last_iter_time).unwrap();

      if delta.as_millis() < ((1000 + event_tick_rate - 1) / event_tick_rate) as u128 {
        if event_tick_rate < 500 {
          std::thread::yield_now();
        } else {
          continue;
        }
      }
      last_iter_time = now;

      let event = self.platform.handle_events();
      if event == PlatformEvent::Quit {
        break 'event_loop;
      }
      renderer.set_window_state(self.platform.window().state());
      renderer.primary_camera().update_rotation(fps_camera_rotation::<P>(self.platform.input(), &mut fps_camera, delta.as_secs_f32()));
    }
  }
}
