#![allow(dead_code)]
extern crate sdl2;
extern crate sdl2_sys;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate bitset_core;
#[macro_use]
extern crate lazy_static;

use sourcerenderer_engine::Engine;
use sourcerenderer_core::platform::{GraphicsApi, PlatformEvent, WindowState, Window};

pub use sdl_platform::SDLPlatform;
use std::time::SystemTime;
use sourcerenderer_core::Platform;

mod sdl_platform;
mod input;
mod io;

const EVENT_TICK_RATE: u32 = 512;

fn main() {
  let mut platform = SDLPlatform::new(GraphicsApi::Vulkan);
  let mut engine = Box::new(Engine::run(platform.as_ref()));
  let mut last_iter_time = SystemTime::now();
  'event_loop: loop {
    let now = SystemTime::now();
    let delta = now.duration_since(last_iter_time).unwrap();

    if delta.as_millis() < ((1000 + EVENT_TICK_RATE - 1) / EVENT_TICK_RATE) as u128 {
      if EVENT_TICK_RATE < 500 {
        std::thread::yield_now();
      } else {
        continue;
      }
    }
    last_iter_time = now;

    let event = platform.handle_events();
    if event == PlatformEvent::Quit {
      break 'event_loop;
    }
    let window_state = platform.window().state();
    engine.update_window_state(window_state);
    let input_commands = engine.receive_input_commands();
    let input = platform.process_input(input_commands);
    engine.update_input_state(input);
  }
}
