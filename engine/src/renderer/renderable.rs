use sourcerenderer_core::Platform;
use std::sync::Arc;
use nalgebra::Matrix4;
use crate::asset::AssetKey;
use sourcerenderer_core::graphics::Backend as GraphicsBackend;

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
pub enum Renderable {
  Static(StaticModelRenderable),
  Skinned // TODO
}

#[derive(Clone)]
pub struct TransformedRenderable {
  pub renderable: Renderable,
  pub transform: Matrix4<f32>,
  pub old_transform: Matrix4<f32>
}

#[derive(Clone)]
pub struct Renderables {
  pub elements: Vec<TransformedRenderable>,
  pub camera: Matrix4<f32>,
  pub old_camera: Matrix4<f32>
}