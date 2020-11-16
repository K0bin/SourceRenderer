use std::sync::Arc;
use sourcerenderer_core::platform::{Input, Key};
use crate::Transform;
use crate::Camera;
use sourcerenderer_core::{Quaternion, Vec3, Platform};
use nalgebra::Unit;
use legion::systems::Builder;
use legion::{component, World};

use crate::scene::DeltaTime;
use crate::renderer::PrimaryCamera;

pub fn install<P: Platform>(_world: &mut World, systems: &mut Builder) {
  systems.add_system(fps_camera_movement_system::<P>());
}

pub struct FpsCameraComponent {}

pub struct FPSCamera {
  pitch: f32,
  yaw: f32
}

impl FPSCamera {
  pub fn new() -> Self {
    FPSCamera {
      pitch: 0f32,
      yaw: 0f32
    }
  }
}

pub fn fps_camera_rotation<P: Platform>(input: &Arc<P::Input>, fps_camera: &mut FPSCamera, delta_time: f32) -> Quaternion {
  // TODO delta timing

  input.toggle_mouse_lock(true);
  let mouse_delta = input.mouse_position();
  fps_camera.pitch -= (mouse_delta.y as f32 / 1000f32) * delta_time;
  fps_camera.yaw -= (mouse_delta.x as f32 / 1000f32) * delta_time;

  Quaternion::from_euler_angles(fps_camera.pitch, fps_camera.yaw, 0f32)
}

#[system(for_each)]
#[filter(component::<Camera>() & component::<FpsCameraComponent>())]
fn fps_camera_movement<P: Platform>(#[resource] input: &Arc<P::Input>, transform: &mut Transform, #[resource] fps_camera: &Arc<PrimaryCamera<P::GraphicsBackend>>, #[resource] delta_time: &DeltaTime) {
  let rotation = fps_camera.rotation();
  let (_, yaw, _) = rotation.euler_angles();

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
    movement_vector = Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(0.0f32, 1.0f32, 0.0f32)), yaw).transform_vector(&movement_vector);
    transform.position += movement_vector * 3f32 * delta_time.secs();
  }
}
