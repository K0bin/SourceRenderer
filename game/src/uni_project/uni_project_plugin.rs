#[cfg(not(target_arch = "wasm32"))]
use crate::uni_project::ui::UIPlugin;
use crate::uni_project::{make_volume_material, manix_transform, MANIX_PATH, TRANSFER_FUNCTION_PATH};
use crate::{RendererPicker, fps_camera};
use bevy_app::{App, Plugin};
use bevy_math::Affine3A;
use sourcerenderer_core::platform::PlatformIO;
use sourcerenderer_engine::VolumeDrawableTransparencyMode;
use sourcerenderer_engine::renderer::{RendererType, VolumeMeshInstance};
use sourcerenderer_engine::transform::InterpolatedTransform;
use std::marker::PhantomData;
use sourcerenderer_engine::asset::{AssetData, AssetLoadPriority, AssetManagerECSResource};
/* TODO:
 * - DLSS/FSR/XeSS/MetalFX
 * - DearImgui controls
 *   - add and remove thresholds
 *   - adjust transfer function gradient for roughness and color
 *   - pick background
 * - optimize marching cubes with min/max lods and indirect dispatch to remove empty/full cells
 */

pub struct UniProjectPlugin<IO: PlatformIO>(PhantomData<IO>);

impl<IO: PlatformIO> Default for UniProjectPlugin<IO> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<IO: PlatformIO> Plugin for UniProjectPlugin<IO> {
    fn build(&self, app: &mut App) {
        {
            log::info!("Initializing university project plugin");
            let model_matrix = manix_transform();

            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: MANIX_PATH.to_string(),
                    material_path: "default0".to_string(),
                    volume_texture_lod: 1,
                    threshold_min: 0.0288f32,
                    transparent: VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque,
                    render_as_cubes: false,
                    ray_march_normals: false,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));
            app.world_mut().spawn((
                VolumeMeshInstance {
                    volume_texture_path: MANIX_PATH.to_string(),
                    material_path: "default0".to_string(),
                    volume_texture_lod: 1,
                    threshold_min: 0.55f32,
                    transparent: VolumeDrawableTransparencyMode::Opaque,
                    render_as_cubes: false,
                    ray_march_normals: false,
                },
                InterpolatedTransform(Affine3A::from_mat4(model_matrix)),
            ));

            #[cfg(not(target_arch = "wasm32"))]
            app.add_plugins(UIPlugin);
        }

        fps_camera::install(app);
    }
}

impl<IO: PlatformIO> RendererPicker for UniProjectPlugin<IO> {
    fn pick_renderer() -> RendererType {
        RendererType::VolumeUniProject
    }
}
