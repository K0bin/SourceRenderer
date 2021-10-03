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
use sourcerenderer_core::platform::GraphicsApi;

pub use sdl_platform::SDLPlatform;

mod sdl_platform;

fn main() {
  Engine::<SDLPlatform>::initialize_global();
  let mut platform = SDLPlatform::new(GraphicsApi::Vulkan);
  let engine = Box::new(Engine::run(platform.as_ref()));

  'event_loop: loop {
    if !engine.is_running() {
      break;
    }

    if !platform.poll_events(&engine) {
      break 'event_loop;
    }
    if engine.is_mouse_locked() {
      platform.reset_mouse_position();
    }
  }
}
