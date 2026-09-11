mod ui;
mod uni_project_plugin;

use sourcerenderer_core::{Matrix4, Vec3};
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
