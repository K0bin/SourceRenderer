use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
#[derive(Component)]
pub struct Camera {
    pub fov: f32,
    pub interpolate_rotation: bool,
}
#[derive(Resource)]
pub struct ActiveCamera(pub Entity);
