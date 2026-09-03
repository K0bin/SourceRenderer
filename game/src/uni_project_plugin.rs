use std::sync::Arc;

use crate::{RendererPicker, fps_camera};
use bevy_app::{App, Plugin};
use bevy_math::Affine3A;
use sourcerenderer_core::{Matrix4, Vec3};
use sourcerenderer_engine::renderer::{RendererType, VolumeMeshInstance};
use sourcerenderer_engine::transform::InterpolatedTransform;
use sourcerenderer_engine::{
    Engine,
    asset::{AssetLoadPriority, AssetManager, AssetType},
};

#[derive(Default)]
pub struct UniProjectPlugin;

impl Plugin for UniProjectPlugin {
    fn build(&self, app: &mut App) {
        {
            log::info!("Initializing university project plugin");
            let marching_cube_scale =
                Vec3::new(0.488281f32, 0.488281f32, 0.700012f32) * 8f32 * 0.01f32;
            let model_matrix = Matrix4::from_rotation_x(-1.57f32)
                * Matrix4::from_rotation_z(3.14)
                * Matrix4::from_scale(marching_cube_scale);

            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: "assets/manix.raw.txt".to_string(),
                    transfer_function_texture_path: "assets/transferfunction.png".to_string(),
                    volume_texture_lod: 3,
                    threshold_max: f32::MAX,
                    threshold_min: 0.0288f32,
                    transparent: true,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));
            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: "assets/manix.raw.txt".to_string(),
                    transfer_function_texture_path: "assets/transferfunction.png".to_string(),
                    volume_texture_lod: 3,
                    threshold_max: f32::MAX,
                    threshold_min: 0.55f32,
                    transparent: false,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));
        }

        fps_camera::install(app);
    }
}

impl RendererPicker for UniProjectPlugin {
    fn pick_renderer() -> RendererType {
        RendererType::VolumeUniProject
    }
}
