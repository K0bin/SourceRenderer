#![allow(dead_code)]
extern crate sourcerenderer_sdl;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_core;

use sourcerenderer_sdl::SDLPlatform;
use sourcerenderer_engine::Engine;
use sourcerenderer_core::platform::GraphicsApi;

fn main() {
  let platform: Box<SDLPlatform> = Box::new(SDLPlatform::new(GraphicsApi::Vulkan));
  let mut engine = Box::new(Engine::new(platform));
  engine.run();
}
