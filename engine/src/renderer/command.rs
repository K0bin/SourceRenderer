use legion::Entity;
use crate::renderer::Drawable;
use nalgebra::Matrix4;

pub enum RendererCommand {
  Register(Drawable),
  UnregisterStatic(Entity),
  UpdateTransform{ entity: Entity, transform_mat: Matrix4<f32> },
  UpdateCameraTransform { camera_transform_mat: Matrix4<f32>, fov: f32 },
  EndFrame
}
