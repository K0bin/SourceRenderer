use crate::uni_project::ui::UIPlugin;
use crate::uni_project::{MANIX_PATH, TRANSFER_FUNCTION_PATH, manix_transform};
use crate::{RendererPicker, fps_camera};
use bevy_app::{App, Plugin};
use bevy_math::Affine3A;
use sourcerenderer_core::{Matrix4, Vec3};
use sourcerenderer_engine::renderer::{RendererType, VolumeMeshInstance};
use sourcerenderer_engine::transform::InterpolatedTransform;
/* TODO:
 * - DLSS/FSR/XeSS/MetalFX
 * - DearImgui controls
 *   - add and remove thresholds
 *   - adjust transfer function gradient for roughness and color
 *   - pick background
 * - optimize marching cubes with min/max lods and indirect dispatch to remove empty/full cells
 */

#[derive(Default)]
pub struct UniProjectPlugin;

impl Plugin for UniProjectPlugin {
    fn build(&self, app: &mut App) {
        {
            log::info!("Initializing university project plugin");
            let model_matrix = manix_transform();

            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: MANIX_PATH.to_string(),
                    transfer_function_texture_path: TRANSFER_FUNCTION_PATH.to_string(),
                    volume_texture_lod: 3,
                    threshold_min: 0.0288f32,
                    transparent: true,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));
            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: MANIX_PATH.to_string(),
                    transfer_function_texture_path: TRANSFER_FUNCTION_PATH.to_string(),
                    volume_texture_lod: 3,
                    threshold_min: 0.55f32,
                    transparent: false,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));

            app.add_plugins(UIPlugin);
        }

        fps_camera::install(app);
    }
}

impl RendererPicker for UniProjectPlugin {
    fn pick_renderer() -> RendererType {
        RendererType::VolumeUniProject
    }
}
