extern crate num_cpus;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate async_std;
extern crate image;
extern crate crossbeam_channel;

pub use self::engine::Engine;
pub use self::renderer::Renderer;
use sourcerenderer_core::{Vec3, Vec2};

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

mod renderer;
mod scene;
