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
