pub use camera::{
    ActiveCamera,
    Camera,
};

pub use self::engine::Engine;
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

#[allow(unused_imports)]
#[cfg(feature = "threading")]
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[cfg(feature = "threading")]
use std::sync::{Mutex, MutexGuard, Condvar};

mod rw_lock_wasm;

#[cfg(not(feature = "threading"))]
use rw_lock_wasm::*;
