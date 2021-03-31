use sourcerenderer_core::{Matrix4};

use legion::Entity;
use std::sync::Arc;
use std::f32;
use sourcerenderer_core::graphics::Backend;
use crate::renderer::renderer_assets::*;
use bitvec::vec::BitVec;

pub enum DrawableType {
  Static {
    model_path: String,
    receive_shadows: bool,
    cast_shadows: bool,
    can_move: bool
  },
  Skinned // TODO
}

impl Clone for DrawableType {
  fn clone(&self) -> Self {
    match self {
      DrawableType::Static {
        model_path, receive_shadows, cast_shadows, can_move
      } => {
        Self::Static {
          model_path: model_path.clone(),
          receive_shadows: *receive_shadows,
          cast_shadows: *cast_shadows,
          can_move: *can_move
        }
      },
      _ => unimplemented!()
    }
  }
}

#[derive(Clone)]
pub struct Drawable {
  pub(super) drawable_type: DrawableType,
  pub(super) entity: Entity,
  pub(super) transform: Matrix4
}

impl Drawable {
  pub fn new(entity: Entity, drawable_type: DrawableType, transform: Matrix4) -> Self {
    Self {
      entity,
      drawable_type,
      transform
    }
  }
}

pub(super) enum RDrawableType<B: Backend> {
  Static {
    model: Arc<RendererModel<B>>,
    receive_shadows: bool,
    cast_shadows: bool,
    can_move: bool
  },
  Skinned // TODO
}

impl<B: Backend> Clone for RDrawableType<B> {
  fn clone(&self) -> Self {
    match self {
      RDrawableType::Static {
        model, receive_shadows, cast_shadows, can_move
      } => {
        Self::Static {
          model: model.clone(),
          receive_shadows: *receive_shadows,
          cast_shadows: *cast_shadows,
          can_move: *can_move
        }
      },
      _ => unimplemented!()
    }
  }
}

#[derive(Clone)]
pub(super) struct RDrawable<B: Backend> {
  pub(super) drawable_type: RDrawableType<B>,
  pub(super) entity: Entity,
  pub(super) transform: Matrix4,
  pub(super) old_transform: Matrix4
}

impl<B: Backend> RDrawable<B> {
  pub fn new(entity: Entity, drawable_type: RDrawableType<B>, transform: Matrix4) -> Self {
    Self {
      entity,
      drawable_type,
      transform,
      old_transform: transform
    }
  }
}

#[derive(Clone)]
pub(crate) struct View {
  pub(super) visible_drawables: BitVec,
  pub(super) camera_matrix: Matrix4,
  pub(super) old_camera_matrix: Matrix4,
  pub(super) camera_transform: Matrix4,
  pub(super) camera_fov: f32
}

impl Default for View {
  fn default() -> Self {
    Self {
      visible_drawables: BitVec::new(),
      camera_transform: Matrix4::identity(),
      old_camera_matrix: Matrix4::identity(),
      camera_matrix: Matrix4::identity(),
      camera_fov: f32::consts::PI / 2f32
    }
  }
}
