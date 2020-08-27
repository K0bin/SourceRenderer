extern crate num_cpus;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate async_std;
extern crate image;
extern crate crossbeam_channel;
extern crate crossbeam_queue;

pub use self::engine::Engine;
pub use self::msg::RendererMessage;
pub use self::msg::GameplayMessage;
pub use self::msg::PhysicsMessage;
pub use self::renderer::Renderer;
pub use self::asset_manager::AssetManager;
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
pub mod asset;
mod msg;
pub mod engine_old;
mod asset_manager;

mod renderer;
mod scene;
