use crate::renderer::drawable::VolumeDrawableTransparencyMode;
use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use bevy_math::Vec3;
use crate::renderer::ecs::AmbientOcclusionOption::VBAO;

#[derive(Clone, Debug, PartialEq, Component)]
pub struct StaticRenderableComponent {
    pub model_path: String,
    pub receive_shadows: bool,
    pub cast_shadows: bool,
    pub can_move: bool,
}

#[derive(Clone, Debug, PartialEq, Component)]
pub struct PointLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq, Component)]
pub struct DirectionalLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Component)]
pub struct Lightmap {
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Component)]
pub struct VolumeMeshInstance {
    pub volume_texture_path: String,
    pub volume_texture_lod: u32,
    pub material_path: String,
    pub threshold_min: f32,
    pub transparent: VolumeDrawableTransparencyMode,
    pub render_as_cubes: bool,
    pub ray_march_normals: bool,
    pub slice_cut: Vec3
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AmbientOcclusionOption {
    None,
    SSAO,
    VBAO,
}

#[derive(Clone, Debug, PartialEq, Resource)]
pub struct VolumeRendererOptions {
    pub background_hdri: Option<String>,
    pub use_mesh_shader: bool,
    pub culling: bool,
    pub sss_width: f32,
    pub ambient_occlusion: AmbientOcclusionOption,
}

impl Default for VolumeRendererOptions {
    fn default() -> Self {
        Self {
            background_hdri: None,
            use_mesh_shader: false,
            culling: true,
            ambient_occlusion: AmbientOcclusionOption::VBAO,
            // 1.5cm (0.015) is typical for human skin but that assumes 1 = 1m and a head actually has the
            // dimensions of a head. Our models can end up being much larger so pick something big.
            sss_width: 0.5,
        }
    }
}
