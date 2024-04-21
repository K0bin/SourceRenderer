#![allow(dead_code)]
extern crate bitset_core;
extern crate raw_window_handle;
extern crate sdl2;
extern crate sdl2_sys;
extern crate sourcerenderer_core;
extern crate sourcerenderer_engine;
extern crate sourcerenderer_vulkan;
#[macro_use]
extern crate lazy_static;

pub use sdl_platform::SDLPlatform;
use sourcerenderer_engine::Engine;

mod sdl_platform;

#[cfg(target_os = "windows")]
mod sdl_vulkan;
#[cfg(target_os = "windows")]
pub(crate) use sdl_vulkan as sdl_gpu;

#[cfg(target_os = "linux")]
mod sdl_vulkan;
#[cfg(target_os = "linux")]
pub(crate) use sdl_vulkan as sdl_gpu;

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();
    //std::thread::sleep(std::time::Duration::from_secs(20));

    Engine::<SDLPlatform>::initialize_global();
    let mut platform = SDLPlatform::new();
    let engine = Box::new(Engine::run(platform.as_ref()));

    'event_loop: loop {
        if !engine.is_running() {
            break;
        }

        if !platform.poll_events(&engine) {
            break 'event_loop;
        }

        platform.update_mouse_lock(engine.is_mouse_locked());

        engine.frame();
    }
    engine.stop();
}
