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
use sourcerenderer_core::platform::{GraphicsApi, PlatformEvent};

pub use sdl_platform::SDLPlatform;
use std::time::SystemTime;


mod sdl_platform;
mod input;
mod io;

const EVENT_TICK_RATE: u32 = 512;

fn main() {
  Engine::<SDLPlatform>::initialize_global();
  let platform = SDLPlatform::new(GraphicsApi::Vulkan);
  let mut engine = Box::new(Engine::run(platform));
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

    let event = engine.platform().handle_events();
    if event == PlatformEvent::Quit {
      break 'event_loop;
    }
    let input_commands = engine.receive_input_commands();
    engine.platform().process_input(input_commands);
    engine.poll_platform();
  }
}
