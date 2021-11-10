use std::sync::Arc;

use sourcerenderer_core::{Quaternion, Vec3, atomic_refcell::AtomicRefCell, graphics::Backend};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PointLight {
  pub position: Vec3,
  pub intensity: f32
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct DirectionalLight {
  pub direction: Vec3,
  pub intensity: f32
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CullingPointLight {
  pub position: Vec3,
  pub radius: f32
}

#[derive(Debug, Clone)]
pub struct RendererDirectionalLight<B: Backend> {
  pub direction: Vec3,
  pub intensity: f32,
  pub shadow_map: AtomicRefCell<Option<Arc<B::Texture>>>
}

impl<B: Backend> RendererDirectionalLight<B> {
  pub fn new(direction: Vec3, intensity: f32) -> Self {
    Self {
      direction,
      intensity,
      shadow_map: AtomicRefCell::new(None)
    }
  }
}


#[derive(Debug, Clone)]
pub struct RendererPointLight<B: Backend> {
  pub position: Vec3,
  pub intensity: f32,
  pub shadow_map: AtomicRefCell<Option<Arc<B::Texture>>>
}

impl<B: Backend> RendererPointLight<B> {
  pub fn new(position: Vec3, intensity: f32) -> Self {
    Self {
      position,
      intensity,
      shadow_map: AtomicRefCell::new(None)
    }
  }
}
