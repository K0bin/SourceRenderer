#![allow(dead_code)]
extern crate sdl2;
extern crate sdl2_sys;
extern crate ash;
extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_vulkan;
extern crate bitset_core;

use sourcerenderer_core::platform::GraphicsApi;
use sourcerenderer_engine::Engine;

use sdl_platform::SDLPlatform;
use input::SDLInput;

mod sdl_platform;
mod input;

fn main() {
  let platform: Box<SDLPlatform> = Box::new(SDLPlatform::new(GraphicsApi::Vulkan));
  let mut engine = Box::new(Engine::new(platform));
  engine.run();
}
