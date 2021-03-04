use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use super::GlobalTransform;
use legion::systems::{Builder, CommandBuffer};
use legion::{Entity, component, maybe_changed};
use crate::scene::{TickDuration, TickDelta};
use nalgebra::Matrix3;

pub struct PreviousGlobalTransform(pub Matrix4);
pub struct InterpolatedTransform(pub Matrix4);

pub fn install(fixed_rate_systems: &mut Builder, systems: &mut Builder) {
  fixed_rate_systems.add_system(update_previous_global_transform_system());

  systems.add_system(interpolate_transform_system());
  systems.add_system(interpolate_new_transform_system());
  systems.flush();
}

#[system(for_each)]
#[filter(maybe_changed::<GlobalTransform>())]
fn update_previous_global_transform(transform: &GlobalTransform,
                                    entity: &Entity,
                                    command_buffer: &mut CommandBuffer) {
  command_buffer.add_component(*entity, PreviousGlobalTransform(transform.0));
}

#[system(for_each)]
#[filter(maybe_changed::<GlobalTransform>())]
fn interpolate_transform(
  transform: &GlobalTransform,
  previous_transform: &PreviousGlobalTransform,
  #[resource] tick_duration: &TickDuration,
  #[resource] tick_delta: &TickDelta,
  entity: &Entity,
  command_buffer: &mut CommandBuffer) {
  let frac = tick_delta.0.as_secs_f32() / tick_duration.0.as_secs_f32();
  let interpolated = interpolate_transform_matrix(&previous_transform.0, &transform.0, frac);
  command_buffer.add_component(*entity, InterpolatedTransform(interpolated));
}

#[system(for_each)]
#[filter(!component::<PreviousGlobalTransform>())]
fn interpolate_new_transform(
  transform: &GlobalTransform,
  entity: &Entity,
  command_buffer: &mut CommandBuffer) {
  command_buffer.add_component(*entity, InterpolatedTransform(transform.0));
}

fn deconstruct_transform(transform_mat: &Matrix4) -> (Vec3, Quaternion, Vec3) {
  let scale = Vec3::new(transform_mat.column(0).xyz().magnitude(),
                        transform_mat.column(1).xyz().magnitude(),
                        transform_mat.column(2).xyz().magnitude());
  let translation: Vec3 = transform_mat.column(3).xyz();
  let rotation = Quaternion::from_matrix(&Matrix3::<f32>::from_columns(&[
    transform_mat.column(0).xyz() / scale.x,
    transform_mat.column(1).xyz() / scale.y,
    transform_mat.column(2).xyz() / scale.z
  ]));
  (translation, rotation, scale)
}

fn interpolate_transform_matrix(from: &Matrix4, to: &Matrix4, frac: f32) -> Matrix4 {
  let (from_position, from_rotation, from_scale) = deconstruct_transform(from);
  let (to_position, to_rotation, to_scale) = deconstruct_transform(to);
  let position = from_position.lerp(&to_position, frac);
  let rotation: Quaternion = Quaternion::from_quaternion(from_rotation.lerp(&to_rotation, frac));
  let scale = from_scale.lerp(&to_scale, frac);

  Matrix4::new_translation(&position)
    * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount))
    * Matrix4::new_nonuniform_scaling(&scale)
}
