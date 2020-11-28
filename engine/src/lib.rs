#![allow(dead_code)]
extern crate num_cpus;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate async_std;
extern crate image;
extern crate crossbeam_channel;
extern crate crossbeam_utils;
extern crate sourcerenderer_bsp;
extern crate sourcerenderer_vpk;
#[macro_use]
extern crate legion;
extern crate regex;

pub use self::engine::Engine;
use sourcerenderer_core::{Vec3, Vec2};
pub use transform::Transform;
pub use transform::Parent;
pub use camera::Camera;
pub use camera::ActiveCamera;
pub use self::bufreader::BufReader;

// TODO move somewhere else
#[repr(C)]
#[derive(Clone, PartialEq, Debug)]
pub struct Vertex {
  pub position: Vec3,
  pub normal: Vec3,
  pub color: Vec3,
  pub uv: Vec2
}

mod engine;
mod asset;
mod spinning_cube;
mod transform;
mod camera;
pub mod fps_camera;

mod renderer;
mod scene;
mod bufreader;
