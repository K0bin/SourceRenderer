#![allow(dead_code)]
extern crate sourcerenderer_sdl;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_core;

use sourcerenderer_sdl::SDLPlatform;
use sourcerenderer_engine::Engine;
use sourcerenderer_core::platform::GraphicsApi;

fn main() {
  let platform: Box<SDLPlatform> = SDLPlatform::new(GraphicsApi::Vulkan);
  let mut engine = Engine::new(platform);
  engine.run();
}
