mod uni_project_plugin;
#[cfg(not(target_arch = "wasm32"))]
mod ui;

use std::sync::Arc;
use bevy_ecs::entity::Entity;
use sourcerenderer_core::{Matrix4, Vec3, Vec4};
use sourcerenderer_engine::asset::{AssetData, AssetLoadPriority, AssetManager, MaterialData};
pub use uni_project_plugin::*;

const MANIX_PATH: &'static str = "assets/manix.raw.txt";
const TRANSFER_FUNCTION_PATH: &'static str = "assets/transferfunction.png";

pub(crate) fn manix_transform() -> Matrix4 {
    Matrix4::from_rotation_x(-1.57f32)
        * Matrix4::from_rotation_z(3.14)
        * Matrix4::from_scale(manix_scale())
}

pub(crate) fn manix_scale() -> Vec3 {
    Vec3::new(0.488281f32, 0.488281f32, 0.700012f32) * 8f32 * 0.01f32
}

fn make_volume_material(path: &str) -> MaterialData {
    let albedo_path = format!("{}_albedo", path);
    let roughness_path = format!("{}_roughness", path);
    let metalness_path = format!("{}_metalnness", path);

    MaterialData::SimplePBR {
        albedo_path: Some(albedo_path),
        roughness_path: Some(roughness_path),
        metalness_path: Some(metalness_path),
        albedo_color: Vec4::new(1.0f32, 1.0f32, 1.0f32, 1.0f32),
        roughness_factor: 1.0f32,
        metalness_factor: 1.0f32,
    }
}
