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

mod sdl_platform;
mod input;
mod io;

fn main() {
  Engine::<SDLPlatform>::initialize_global();
  let platform = SDLPlatform::new(GraphicsApi::Vulkan);
  let mut engine = Box::new(Engine::run(platform));
  'event_loop: loop {
    let event = engine.platform().handle_events();
    if event == PlatformEvent::Quit {
      break 'event_loop;
    }
    let input_commands = engine.receive_input_commands();
    engine.platform().process_input(input_commands);
    engine.poll_platform();
  }
}
