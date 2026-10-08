mod uni_project_plugin;
#[cfg(not(target_arch = "wasm32"))]
mod ui;

use sourcerenderer_core::{Matrix4, Vec3, Vec4};
use sourcerenderer_engine::asset::MaterialData;
pub use uni_project_plugin::*;

const MANIX_PATH: &'static str = "assets/ct/manix/manix.raw.txt";
const MECANIX_PATH: &'static str = "assets/ct/mecanix/mecanix.raw.txt";
const DENTAL1_PATH: &'static str = "assets/ct/dental1/dental1.raw.txt";
const LUNGS_PATH: &'static str = "assets/ct/lungs/lungs.raw.txt";
const ARTICULATION_PATH: &'static str = "assets/ct/articulation/articulation.raw.txt";
const CT_HEAD_PATH: &'static str = "assets/ct/CT_HEAD/CT_HEAD.raw.txt";

const MESHES: [&'static str; 6] = [
    MANIX_PATH,
    MECANIX_PATH,
    DENTAL1_PATH,
    LUNGS_PATH,
    ARTICULATION_PATH,
    CT_HEAD_PATH
];

const TRANSFER_FUNCTION_PATH: &'static str = "assets/transferfunction.png";

pub(crate) fn mesh_transform(index: usize) -> Matrix4 {
    Matrix4::from_rotation_x(-1.57f32)
        * Matrix4::from_rotation_z(3.14)
        * Matrix4::from_scale(mesh_scale(index))
}

const MESH_SCALING: [Vec3; 6] = [
    Vec3::new(0.488281f32, 0.488281f32, 0.700012f32),
    Vec3::new(0.558594, 0.558594, 0.799988),
    Vec3::new(0.290000, 0.290000, 0.315733),
    Vec3::new(0.782000, 0.782000, 0.400000),
    Vec3::new(0.907000, 0.907000, 0.300000),
    Vec3::new(0.426000, 0.426000, 0.300000),
];

pub(crate) fn mesh_scale(index: usize) -> Vec3 {
    MESH_SCALING[index] * 8f32 * 0.01f32
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
