use std::sync::Arc;
use sourcerenderer_core::platform::{Input, Key};
use crate::Transform;
use crate::Camera;
use sourcerenderer_core::{Quaternion, Vec3, Platform, Vec2};
use nalgebra::Unit;
use legion::systems::Builder;
use legion::{component, World};

use crate::scene::{DeltaTime, TickRate};
use crate::renderer::LateLatchCamera;

pub fn install<P: Platform>(_world: &mut World, systems: &mut Builder) {
  systems.add_system(retrieve_fps_camera_rotation_system::<P>());
  systems.add_system(fps_camera_movement_system::<P>());
  systems.add_system(update_fps_camera_position_system::<P>());
}

pub struct FPSCameraComponent {
}

pub struct FPSCamera {
  sensitivity: f32,
  pitch: f32,
  yaw: f32,
  last_touch_position: Vec2
}

impl FPSCamera {
  pub fn new() -> Self {
    FPSCamera {
      sensitivity: 1.0f32,
      pitch: 0f32,
      yaw: 0f32,
      last_touch_position: Vec2::new(0f32, 0f32)
    }
  }
}

pub fn fps_camera_rotation<P: Platform>(input: &Arc<P::Input>, fps_camera: &mut FPSCamera, _delta_time: f32) -> Quaternion {
  input.toggle_mouse_lock(true);
  let mouse_delta = input.mouse_position();
  let touch_position = input.finger_position(0);
  let touch_delta = if fps_camera.last_touch_position.x.abs() > 0.1f32 && fps_camera.last_touch_position.y.abs() > 0.1f32 {
    touch_position - fps_camera.last_touch_position
  } else {
    Vec2::new(0f32, 0f32)
  };
  fps_camera.pitch += mouse_delta.y as f32 / 20_000f32 * fps_camera.sensitivity;
  fps_camera.yaw -= mouse_delta.x as f32 / 20_000f32 * fps_camera.sensitivity;
  fps_camera.pitch += touch_delta.y / 20_000f32 * fps_camera.sensitivity;
  fps_camera.yaw -= touch_delta.x / 20_000f32 * fps_camera.sensitivity;

  fps_camera.pitch = fps_camera.pitch.max(-std::f32::consts::FRAC_PI_2 + 0.01f32).min(std::f32::consts::FRAC_PI_2 - 0.01f32);

  fps_camera.last_touch_position = touch_position;
  Quaternion::from_euler_angles(fps_camera.pitch, fps_camera.yaw, 0f32)
}

#[system(for_each)]
#[filter(component::<Camera>() & component::<FPSCameraComponent>())]
fn retrieve_fps_camera_rotation<P: Platform>(#[resource] late_latch_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>, transform: &mut Transform) {
  transform.rotation = late_latch_camera.rotation();
}

#[system(for_each)]
#[filter(component::<Camera>() & component::<FPSCameraComponent>())]
fn update_fps_camera_position<P: Platform>(#[resource] late_latch_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>, transform: &mut Transform) {
  late_latch_camera.update_position(transform.position);
}

#[system(for_each)]
#[filter(component::<Camera>() & component::<FPSCameraComponent>())]
fn fps_camera_movement<P: Platform>(#[resource] input: &Arc<P::Input>, transform: &mut Transform, #[resource] tick_rate: &TickRate) {
  let mut movement_vector = Vec3::new(0f32, 0f32, 0f32);
  if input.is_key_down(Key::W) {
    movement_vector.z += 1f32;
  }
  if input.is_key_down(Key::S) {
    movement_vector.z -= 1f32;
  }
  if input.is_key_down(Key::A) {
    movement_vector.x -= 1f32;
  }
  if input.is_key_down(Key::D) {
    movement_vector.x += 1f32;
  }
  if input.is_key_down(Key::Q) {
    movement_vector.y += 1f32;
  }
  if input.is_key_down(Key::E) {
    movement_vector.y -= 1f32;
  }

  if movement_vector.x.abs() > 0.00001f32 || movement_vector.z.abs() > 0.00001f32 {
    let y = movement_vector.y;
    movement_vector = movement_vector.normalize();
    movement_vector = Vec3::new(-movement_vector.x, 0.0f32, movement_vector.z).normalize();
    movement_vector = transform.rotation.transform_vector(&movement_vector);
    movement_vector.y = y;
  }

  if movement_vector.x.abs() > 0.00001f32 || movement_vector.y.abs() > 0.00001f32 || movement_vector.z.abs() > 0.00001f32 {
    movement_vector = movement_vector.normalize();
    transform.position += movement_vector * 8f32 / (tick_rate.0 as f32);
  }
}
