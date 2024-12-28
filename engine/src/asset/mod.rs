mod asset_manager;
pub mod loaders;
mod loaded_level;

#[derive(Clone, Debug)]
pub struct Vertex {
  pub position: Vec3,
  pub tex_coord: Vec2,
  pub normal: Vec3,
  pub color: [u8; 4],
}

pub use asset_manager::{
    Asset,
    AssetLoadPriority,
    AssetLoader,
    AssetLoaderProgress,
    AssetManager,
    AssetType,
    Material,
    MaterialValue,
    Mesh,
    MeshRange,
    Model,
    Texture,
    AssetContainer,
    DirectlyLoadedAsset,
    AssetContainerAsync,
    AssetLoaderAsync
};

use bevy_math::Vec2;
pub use loaded_level::LoadedLevel;
use sourcerenderer_core::Vec3;
