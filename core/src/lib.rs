extern crate cgmath;
extern crate num_cpus;
#[macro_use]
extern crate bitflags;

pub use self::engine::Engine;
pub use cgmath::{Vector3, Vector4, Matrix4};


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
