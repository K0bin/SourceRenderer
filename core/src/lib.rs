#[macro_use]
extern crate bitflags;
extern crate crossbeam_channel;
extern crate crossbeam_queue;
extern crate crossbeam_deque;
extern crate crossbeam_utils;
extern crate num_cpus;
extern crate nalgebra;

pub mod graphics;
pub mod platform;
pub mod pool;

pub mod job;

pub use crate::platform::Platform;

pub use crate::job::Job;
pub use crate::job::JobCounter;
pub use crate::job::JobScheduler;

pub type Vec2 = nalgebra::Vector2<f32>;
pub type Vec3 = nalgebra::Vector3<f32>;
pub type Vec4 = nalgebra::Vector4<f32>;
pub type Vec2I = nalgebra::Vector2<i32>;
pub type Vec2UI = nalgebra::Vector2<u32>;
