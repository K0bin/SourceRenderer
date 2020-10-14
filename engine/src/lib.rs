extern crate num_cpus;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate async_std;
extern crate image;
extern crate crossbeam_channel;
extern crate sourcerenderer_bsp;
#[macro_use]
extern crate legion;

pub use self::engine::Engine;
use sourcerenderer_core::{Vec3, Vec2};
pub use transform::Transform;
pub use transform::Parent;
pub use camera::Camera;

// TODO move somewhere else
#[repr(C)]
#[derive(Clone, PartialEq, Debug)]
pub struct Vertex {
  pub position: Vec3,
  pub color: Vec3,
  pub uv: Vec2
}

mod engine;
mod asset;
mod spinning_cube;
mod transform;
mod camera;

mod renderer;
mod scene;
