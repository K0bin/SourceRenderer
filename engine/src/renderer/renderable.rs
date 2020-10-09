use sourcerenderer_core::Platform;
use std::sync::Arc;
use nalgebra::Matrix4;
use crate::asset::AssetKey;
use sourcerenderer_core::graphics::Backend as GraphicsBackend;
use legion::Entity;

pub struct StaticModelRenderable {
  pub model: AssetKey,
  pub receive_shadows: bool,
  pub cast_shadows: bool,
  pub can_move: bool
}

impl Clone for StaticModelRenderable {
  fn clone(&self) -> Self {
    Self {
      model: self.model.clone(),
      receive_shadows: self.receive_shadows,
      cast_shadows: self.cast_shadows,
      can_move: self.can_move
    }
  }
}

#[derive(Clone)]
pub enum RenderableType {
  Static(StaticModelRenderable),
  Skinned // TODO
}

#[derive(Clone)]
pub struct Renderable {
  pub renderable_type: RenderableType,
  pub entity: Entity,
  pub transform: Matrix4<f32>,
  pub old_transform: Matrix4<f32>
}

#[derive(Clone)]
pub struct Renderables {
  pub elements: Vec<Renderable>,
  pub camera: Matrix4<f32>,
  pub old_camera: Matrix4<f32>
}

impl Default for Renderables {
  fn default() -> Self {
    Self {
      elements: Vec::new(),
      camera: Matrix4::<f32>::identity(),
      old_camera: Matrix4::<f32>::identity(),
    }
  }
}