mod asset_manager;
pub mod loaders;
mod loaded_level;
mod handle_map;
mod asset_types;
mod asset_data;
mod asset_manager_plugin;

#[derive(Clone, Debug)]
#[repr(C)]
pub struct Vertex {
  pub position: Vec3,
  pub tex_coord: Vec2,
  pub normal: Vec3,
  pub color: [u8; 4],
}

pub use asset_manager::{
    AssetLoadPriority,
    AssetLoader,
    AssetLoaderProgress,
    AssetManager,
    AssetContainer,
};
pub use self::asset_types::*;
pub(crate) use self::handle_map::*;
pub use self::asset_data::*;
pub use self::asset_manager_plugin::*;

use bevy_math::Vec2;
use sourcerenderer_core::Vec3;
