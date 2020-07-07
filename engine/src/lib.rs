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

mod engine;
pub mod asset;
mod msg;
pub mod engine_old;

mod renderer;
mod scene;
