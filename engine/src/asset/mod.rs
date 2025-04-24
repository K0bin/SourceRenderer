mod asset_data;
mod asset_manager;
mod asset_manager_plugin;
mod asset_types;
mod fixed_byte_size_cache;
mod loaded_level;
pub mod loaders;

#[derive(Clone, Debug)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub tex_coord: Vec2,
    pub normal: Vec3,
    pub color: [u8; 4],
}

pub use asset_manager::{
    AssetContainer,
    AssetLoadPriority,
    AssetLoader,
    AssetLoaderProgress,
    AssetManager,
    LoadedAssetData,
};
use bevy_math::Vec2;
pub use fixed_byte_size_cache::*;
use sourcerenderer_core::Vec3;

pub use self::asset_data::*;
pub use self::asset_manager_plugin::*;
pub use self::asset_types::*;
