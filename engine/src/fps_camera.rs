use std::sync::Arc;
use sourcerenderer_core::platform::{Input, Key};
use crate::Transform;
use crate::Camera;
use sourcerenderer_core::{Quaternion, Vec3, Platform, Vec2I};
use nalgebra::Unit;
use legion::systems::Builder;
use legion::{component, World, Entity, IntoQuery};
use crate::transform::GlobalTransform;

pub fn install<P: Platform>(world: &mut World, systems: &mut Builder) {
  systems.add_system(fps_camera_system::<P>());
}

pub struct FPSCameraComponent {
  pitch: f32,
  yaw: f32
}

impl FPSCameraComponent {
  pub fn new() -> Self {
    FPSCameraComponent {
      pitch: 0f32,
      yaw: 0f32
    }
  }
}

#[system(for_each)]
#[filter(component::<Camera>())]
fn fps_camera<P: Platform>(#[resource] input: &Arc<P::Input>, transform: &mut Transform, fps_camera: &mut FPSCameraComponent) {
  // TODO delta timing

  input.toggle_mouse_lock(true);
  let mouse_delta = input.mouse_position();
  fps_camera.pitch -= mouse_delta.y as f32 / 1000f32;
  fps_camera.yaw -= mouse_delta.x as f32 / 1000f32;

  transform.rotation = Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(1.0f32, 0.0f32, 0.0f32)), fps_camera.pitch)
    * Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(0.0f32, 1.0f32, 0.0f32)), fps_camera.yaw);

  let mut movement_vector = Vec3::new(0f32, 0f32, 0f32);
  if input.is_key_down(Key::W) {
    movement_vector.z += 1f32;
  }
  if input.is_key_down(Key::S) {
    movement_vector.z -= 1f32;
  }
  if input.is_key_down(Key::A) {
    movement_vector.x += 1f32;
  }
  if input.is_key_down(Key::D) {
    movement_vector.x -= 1f32;
  }
  if movement_vector.x.abs() > 0.00001f32 || movement_vector.y.abs() > 0.00001f32 || movement_vector.z.abs() > 0.00001f32 {
    movement_vector = movement_vector.normalize();
    movement_vector = Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(0.0f32, 1.0f32, 0.0f32)), fps_camera.yaw).transform_vector(&movement_vector);
    transform.position += movement_vector * 0.01f32;
  }
}
