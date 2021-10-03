#![allow(dead_code)]
#[macro_use]
extern crate bitflags;
extern crate crossbeam_channel;
extern crate crossbeam_deque;
extern crate crossbeam_utils;
extern crate num_cpus;
extern crate nalgebra;
extern crate rayon;
extern crate bitset_core;

pub mod graphics;
pub mod platform;
pub mod pool;
pub mod input;

pub mod atomic_refcell;

pub use crate::platform::Platform;

pub use rayon::ThreadPoolBuilder;
pub use rayon::ThreadPoolBuildError;
pub use rayon::ThreadPool;
pub use rayon::scope;
pub use rayon::spawn;

pub type Vec2 = nalgebra::Vector2<f32>;
pub type Vec3 = nalgebra::Vector3<f32>;
pub type Vec4 = nalgebra::Vector4<f32>;
pub type Vec2I = nalgebra::Vector2<i32>;
pub type Vec2UI = nalgebra::Vector2<u32>;
pub type Quaternion = nalgebra::UnitQuaternion<f32>;
pub type Matrix4 = nalgebra::Matrix4<f32>;
