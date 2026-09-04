pub use camera::{ActiveCamera, Camera};

pub use self::engine::{Engine, EngineLoopFuncResult, WindowState};

pub use bevy_input::ButtonState;
pub use bevy_input::keyboard::{Key, KeyboardInput};
pub use bevy_input::mouse::MouseMotion;
pub use bevy_input::prelude::*;

mod engine;

pub mod asset;
pub mod camera;
pub mod math;
pub mod transform;

mod input;
//mod physics;
mod dear_imgui;
pub mod graphics;
pub mod renderer;

mod async_counter;
pub use async_counter::*;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use std::sync::{Condvar, Mutex, MutexGuard};

#[allow(unused_imports)]
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

mod convenience_inputs;
pub mod tasks;

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn autoreleasepool<T, F>(func: F) -> T
where
    for<'pool> F: objc2::rc::AutoreleaseSafe + FnOnce() -> T,
{
    objc2::rc::autoreleasepool(|_| func())
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn autoreleasepool<T, F: FnOnce() -> T>(func: F) -> T {
    func()
}
