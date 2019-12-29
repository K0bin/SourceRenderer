extern crate sdl2;
extern crate ash;
extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_vulkan;

use std::time::Duration;

use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::platform::Window;
use sourcerenderer_core::platform::GraphicsApi;
use sourcerenderer_engine::Engine;

use sdl_platform::SDLPlatform;

mod sdl_platform;

fn main() {
  let mut platform: Box<SDLPlatform> = Box::new(SDLPlatform::new(GraphicsApi::Vulkan));
  let mut engine = Box::new(Engine::new(platform));
  engine.run();
}
