use legion::Entity;
use crate::renderer::renderable::Renderable;
use nalgebra::Matrix4;

pub enum RendererCommand {
  Register(Renderable),
  UnregisterStatic(Entity),
  UpdateTransform(Entity, Matrix4<f32>),
  UpdateCamera(Matrix4<f32>),
  EndFrame
}
