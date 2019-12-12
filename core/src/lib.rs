extern crate vek;
extern crate num_cpus;
#[macro_use]
extern crate bitflags;

pub use self::engine::Engine;
pub type Vec2 = vek::Vec2<f32>;
pub type Vec3 = vek::Vec3<f32>;
pub type Vec4 = vek::Vec4<f32>;
pub type Vec2I = vek::Vec2<i32>;
pub type Vec2UI = vek::Vec2<u32>;


pub use self::cast::unsafe_arc_cast;
pub use self::cast::unsafe_box_cast;
pub use self::cast::unsafe_ref_cast;
pub use self::cast::unsafe_mut_cast;
pub use self::cast::rc_to_box;

mod engine;
pub mod platform;
pub mod asset;
pub mod job;
pub mod graphics;
mod cast;
