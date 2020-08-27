use sourcerenderer_core::Platform;
use std::sync::Arc;
use nalgebra::Matrix4;
use crate::asset_manager::AssetKey;
use sourcerenderer_core::graphics::Backend as GraphicsBackend;

// TODO: better name
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum Movability {
  Static,
  Dynamic
}

pub struct StaticModelRenderable<P: Platform> {
  pub model: Arc<AssetKey<P>>,
  pub receive_shadows: bool,
  pub cast_shadows: bool,
  pub movability: Movability
}

impl<P: Platform> Clone for StaticModelRenderable<P> {
  fn clone(&self) -> Self {
    Self {
      model: self.model.clone(),
      receive_shadows: self.receive_shadows,
      cast_shadows: self.cast_shadows,
      movability: self.movability
    }
  }
}

#[derive(Clone)]
pub enum Renderable<P: Platform> {
  Static(StaticModelRenderable<P>)
}

#[derive(Clone)]
pub struct RenderableAndTransform<P: Platform> {
  pub renderable: Renderable<P>,
  pub transform: Matrix4<f32>,
  pub old_transform: Matrix4<f32>
}

#[derive(Clone)]
pub struct Renderables<P: Platform> {
  pub elements: Vec<RenderableAndTransform<P>>,
  pub camera: Matrix4<f32>,
  pub old_camera: Matrix4<f32>
}