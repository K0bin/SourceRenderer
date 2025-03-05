use bevy_app::{App, FixedPostUpdate, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::{component::Component, entity::Entity, query::Added, system::{Commands, Query, Res}};
use bevy_math::Affine3A;
use bevy_time::{Fixed, Time};
use bevy_transform::components::{GlobalTransform, Transform};

#[derive(Component)]
pub struct PreviousGlobalTransform(pub Affine3A);

#[derive(Component)]
pub struct InterpolatedTransform(pub Affine3A);

#[derive(Default)]
pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedPostUpdate, update_previous_global_transform);
        app.add_systems(PostUpdate, interpolate_transform_matrix);
    }
}

fn update_previous_global_transform(
    query: Query<(Entity, &GlobalTransform)>,
    mut commands: Commands,
) {
    for (entity, transform) in query.iter() {
        commands.entity(entity).insert(PreviousGlobalTransform(transform.affine()));
    }
}

fn interpolate_transform_matrix(
    time: Res<Time<Fixed>>,
    query: Query<(Entity, &PreviousGlobalTransform, &GlobalTransform)>,
    mut commands: Commands,
) {
    for (entity, old_transform, new_transform) in query.iter() {
        let (old_scale, old_rotation, old_translation) = old_transform.0.to_scale_rotation_translation();
        let (new_scale, new_rotation, new_translation) = new_transform.to_scale_rotation_translation();
        let s = time.overstep_fraction();

        commands.entity(entity).insert(
            InterpolatedTransform(
                Affine3A::from_scale_rotation_translation(
                    old_scale.lerp(new_scale, s), old_rotation.lerp(new_rotation,s), old_translation.lerp(new_translation, s))
            )
        );
    }
}
