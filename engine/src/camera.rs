use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use crate::transform::GlobalTransform;
use legion::systems::{CommandBuffer, System, Builder};
use legion::{Entity, maybe_changed};
use nalgebra::{Point3, Transform3, Translation3, Isometry3, Unit};

pub struct Camera {
  pub fov: f32
}

pub struct GlobalCamera(pub Matrix4);

pub struct ActiveCamera(pub Entity);

pub fn install(systems: &mut Builder) {
  systems.flush();
  systems.add_system(update_camera_global_system());
}

#[system(for_each)]
#[filter(maybe_changed::<GlobalTransform>())]
fn update_camera_global(entity: &Entity,
                        camera: &Camera,
                        global_transform: &GlobalTransform,
                        command_buffer: &mut CommandBuffer) {
  let position = global_transform.0.transform_point(&Point3::new(0.0f32, 0.0f32, 0.0f32));
  let target = global_transform.0.transform_point(&Point3::new(0.0f32, 0.0f32, 1.0f32));

  command_buffer.add_component(*entity, GlobalCamera (
    Matrix4::new_perspective(16f32 / 9f32, 1.02974f32, 0.001f32, 20.0f32)
      * Matrix4::look_at_rh(&position, &target, &Vec3::new(0.0f32, 1.0f32, 0.0f32))
  ));
}