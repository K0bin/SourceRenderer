use sourcerenderer_core::{Matrix4, Vec3};
use crate::transform::GlobalTransform;
use legion::systems::{CommandBuffer, System, Builder};
use legion::Entity;
use nalgebra::Point3;

pub struct Camera {
  pub fov: f32
}

pub struct GlobalCamera(pub Matrix4);

pub fn install(systems: &mut Builder) {
  systems.flush();
  systems.add_system(update_camera_global_system());
}

#[system(for_each)]
fn update_camera_global(entity: &Entity,
                        camera: &Camera,
                        global_transform: &GlobalTransform,
                        command_buffer: &mut CommandBuffer) {
  command_buffer.add_component(*entity, GlobalCamera (
    Matrix4::new_perspective(16f32 / 9f32, 1.02974f32, 0.001f32, 20.0f32)
      * &global_transform.0
  ));
}