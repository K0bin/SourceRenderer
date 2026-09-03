use bevy_ecs::component::Component;

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
    pub transfer_function_texture_path: String,
    pub threshold_min: f32,
    pub threshold_max: f32,
    pub transparent: bool,
}
