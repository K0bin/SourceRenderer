use std::collections::HashSet;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use web_time::Duration;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::{
    Matrix4,
    Platform,
};

use crate::transform::InterpolatedTransform;
use crate::ui::UIDrawData;
use crate::{
    ActiveCamera,
    Camera,
};

use super::renderer::RendererSender;

#[derive(Clone, Debug, PartialEq)]
#[derive(Component)]
pub struct StaticRenderableComponent {
    pub model_path: String,
    pub receive_shadows: bool,
    pub cast_shadows: bool,
    pub can_move: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[derive(Component)]
pub struct PointLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq)]
#[derive(Component)]
pub struct DirectionalLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Component)]
pub struct Lightmap {
    pub path: String,
}

#[derive(Clone, Default, Debug)]
pub struct ActiveStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct ActivePointLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredPointLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct ActiveDirectionalLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredDirectionalLights(HashSet<Entity>);
