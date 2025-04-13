pub use camera::{
    ActiveCamera,
    Camera,
};

pub use self::engine::{
    Engine,
    EngineLoopFuncResult,
};
pub use self::engine::WindowState;

mod engine;

pub mod asset;
pub mod camera;
pub mod math;
pub mod transform;

mod input;
//mod physics;
pub mod renderer;
mod ui;
pub mod graphics;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[allow(unused_imports)]
#[cfg(feature = "threading")]
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[cfg(feature = "threading")]
use std::sync::{Mutex, MutexGuard, Condvar};

mod rw_lock_wasm;

#[cfg(not(feature = "threading"))]
use rw_lock_wasm::*;

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
