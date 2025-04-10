mod asset_manager;
pub mod loaders;
mod loaded_level;
mod asset_types;
mod asset_data;
mod asset_manager_plugin;
mod fixed_byte_size_cache;

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
    LoadedAssetData,
};
pub use self::asset_types::*;
pub use self::asset_data::*;
pub use self::asset_manager_plugin::*;
pub use fixed_byte_size_cache::*;

use bevy_math::Vec2;
use sourcerenderer_core::Vec3;
