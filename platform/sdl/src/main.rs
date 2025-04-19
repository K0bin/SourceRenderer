#[macro_use]
extern crate lazy_static;

pub use sdl_platform::SDLPlatform;
use sdl_platform::StdIO;
use sourcerenderer_engine::{Engine, EngineLoopFuncResult};

mod sdl_platform;
#[cfg(target_os = "macos")]
mod sdl_metal;
#[cfg(target_os = "macos")]
pub(crate) use sdl_metal as sdl_gpu;

#[cfg(target_os = "windows")]
mod sdl_vulkan;
#[cfg(target_os = "windows")]
pub(crate) use sdl_vulkan as sdl_gpu;

#[cfg(target_os = "linux")]
mod sdl_vulkan;
#[cfg(target_os = "linux")]
pub(crate) use sdl_vulkan as sdl_gpu;
use sourcerenderer_game::GamePlugin;

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn autoreleasepool<T, F>(func: F) -> T
    where
        for<'pool> F: objc2::rc::AutoreleaseSafe + FnOnce() -> T {
    objc2::rc::autoreleasepool(|_| func())
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn autoreleasepool<T, F: FnOnce() -> T>(func: F) -> T {
    func()
}

pub fn main() {
    let mut platform = SDLPlatform::new();
    let mut engine = Box::new(Engine::run::<_, StdIO, SDLPlatform>(platform.window(), GamePlugin::<StdIO>::default()));

    'event_loop: loop {
        let engine_loop_result = autoreleasepool(|| {
            if !platform.poll_events(&mut engine) {
                return EngineLoopFuncResult::Exit;
            }

            platform.update_mouse_lock(engine.is_mouse_locked());

            engine.frame()
        });
        if engine_loop_result == EngineLoopFuncResult::Exit {
            break 'event_loop;
        }
    }

    std::process::exit(0);
}
