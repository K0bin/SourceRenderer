#![allow(dead_code)]
extern crate sdl2;
extern crate sdl2_sys;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate bitset_core;

pub use sdl_platform::SDLPlatform;
use input::SDLInput;

mod sdl_platform;
mod input;
