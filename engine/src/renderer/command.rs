use legion::Entity;
use crate::renderer::Drawable;
use nalgebra::Matrix4;

pub enum RendererCommand {
  Register(Drawable),
  UnregisterStatic(Entity),
  UpdateTransform(Entity, Matrix4<f32>),
  UpdateCamera(Matrix4<f32>),
  EndFrame
}
