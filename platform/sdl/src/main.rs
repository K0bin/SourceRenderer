extern crate sdl2;
extern crate sourcerenderer_core;

use std::time::Duration;

use sourcerenderer_core::Platform;
use sourcerenderer_core::Window;
use sourcerenderer_core::Engine;

use sdl_platform::SDLPlatform;

mod sdl_platform;

fn main() {
    let mut platform: Box<SDLPlatform> = Box::new(SDLPlatform::new());
    let mut engine = Box::new(Engine::new(platform));
    engine.run();
}
