use bevy_ecs::{component::Component, entity::Entity, system::Resource};

#[derive(Component)]
pub struct Camera {
    pub fov: f32,
    pub interpolate_rotation: bool,
}

#[derive(Resource)]
pub struct ActiveCamera(pub Entity);
