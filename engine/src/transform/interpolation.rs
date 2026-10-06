use bevy_app::{App, FixedPostUpdate, Plugin, PostUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::{Added, Changed, Or, ParallelCommands};
use bevy_ecs::system::{
    Commands,
    Query,
    Res,
};
use bevy_math::Affine3A;
use bevy_time::{
    Fixed,
    Time,
};
use bevy_transform::components::GlobalTransform;

#[derive(Component)]
pub struct PreviousGlobalTransform(pub Affine3A);

#[derive(Component)]
pub struct InterpolatedTransform(pub Affine3A);

#[derive(Default)]
pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedPostUpdate, add_previous_global_transform);
        app.add_systems(FixedPostUpdate, update_previous_global_transform);
        app.add_systems(PostUpdate, interpolate_transform_matrix);
    }
}

// Add PreviousGlobalTransform component, so we build an InterPolatedTransform later
// without a frame of latency.
fn add_previous_global_transform(
    query: Query<(Entity, &GlobalTransform), Added<GlobalTransform>>,
    mut commands: Commands,
) {
    for (entity, transform) in query.iter() {
        commands
            .entity(entity)
            .insert(PreviousGlobalTransform(transform.affine()));
    }
}

fn update_previous_global_transform(
    query: Query<(Entity, &GlobalTransform), Changed<GlobalTransform>>,
    mut commands: Commands,
) {
    for (entity, transform) in query.iter() {
        commands
            .entity(entity)
            .insert(PreviousGlobalTransform(transform.affine()));
    }
}

#[allow(unused)]
fn interpolate_transform_matrix_mt(
    time: Res<Time<Fixed>>,
    query: Query<(Entity, &PreviousGlobalTransform, &GlobalTransform), Or<(Added<GlobalTransform>, Added<PreviousGlobalTransform>, Changed<GlobalTransform>)>>,
    par_commands: ParallelCommands,
) {
    query.par_iter().for_each(|(entity, old_transform, new_transform)| {
        let (old_scale, old_rotation, old_translation) =
            old_transform.0.to_scale_rotation_translation();
        let (new_scale, new_rotation, new_translation) =
            new_transform.to_scale_rotation_translation();
        let s = time.overstep_fraction();
        par_commands.command_scope(|mut commands| {
            commands.entity(entity).insert(InterpolatedTransform(
                Affine3A::from_scale_rotation_translation(
                    old_scale.lerp(new_scale, s),
                    old_rotation.lerp(new_rotation, s),
                    old_translation.lerp(new_translation, s),
                ),
            ));
        });
    });
}

fn interpolate_transform_matrix(
    time: Res<Time<Fixed>>,
    query: Query<(Entity, &PreviousGlobalTransform, &GlobalTransform), Or<(Added<GlobalTransform>, Changed<GlobalTransform>, Added<PreviousGlobalTransform>)>>,
    mut commands: Commands,
) {
    let batch: Vec<(Entity, InterpolatedTransform)> = query.iter().map(|(entity, old_transform, new_transform)| {
        let (old_scale, old_rotation, old_translation) =
            old_transform.0.to_scale_rotation_translation();
        let (new_scale, new_rotation, new_translation) =
            new_transform.to_scale_rotation_translation();
        let s = time.overstep_fraction();

        (entity, InterpolatedTransform(
            Affine3A::from_scale_rotation_translation(
                old_scale.lerp(new_scale, s),
                old_rotation.lerp(new_rotation, s),
                old_translation.lerp(new_translation, s),
            ),
        ))
    }).collect();
    commands.insert_batch(batch);
}
