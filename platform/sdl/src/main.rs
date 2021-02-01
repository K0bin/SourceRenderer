#![allow(dead_code)]
extern crate sdl2;
extern crate sdl2_sys;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate bitset_core;

use sourcerenderer_engine::Engine;
use sourcerenderer_core::platform::GraphicsApi;

pub use sdl_platform::SDLPlatform;
use input::SDLInput;

mod sdl_platform;
mod input;

fn main() {
  let platform: Box<SDLPlatform> = SDLPlatform::new(GraphicsApi::Vulkan);
  let mut engine = Engine::new(platform);
  engine.run();
}
