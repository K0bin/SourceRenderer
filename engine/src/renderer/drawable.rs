use sourcerenderer_core::{Matrix4};

use crate::asset::AssetKey;

use legion::Entity;
use std::f32;

pub enum DrawableType {
  Static {
    model: AssetKey,
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
pub struct Drawable {
  pub(super) drawable_type: DrawableType,
  pub(super) entity: Entity,
  pub(super) interpolated_transform: Matrix4,
  pub(super) transform: Matrix4,
  pub(super) old_transform: Matrix4,
  pub(super) older_transform: Matrix4
}

impl Drawable {
  pub fn new(entity: Entity, drawable_type: DrawableType, transform: Matrix4) -> Self {
    Self {
      entity,
      drawable_type,
      transform,
      old_transform: transform,
      older_transform: transform,
      interpolated_transform: transform
    }
  }
}

#[derive(Clone)]
pub(crate) struct View {
  pub(super) elements: Vec<Drawable>,
  pub(super) interpolated_camera: Matrix4,
  pub(super) camera_transform: Matrix4,
  pub(super) old_camera_transform: Matrix4,
  pub(super) older_camera_transform: Matrix4,
  pub(super) camera_fov: f32,
  pub(super) old_camera_fov: f32,
  pub(super) older_camera_fov: f32
}

impl Default for View {
  fn default() -> Self {
    Self {
      elements: Vec::new(),
      old_camera_transform: Matrix4::identity(),
      older_camera_transform: Matrix4::identity(),
      camera_transform: Matrix4::identity(),
      interpolated_camera: Matrix4::identity(),
      camera_fov: f32::consts::PI / 2f32,
      old_camera_fov: f32::consts::PI / 2f32,
      older_camera_fov: f32::consts::PI / 2f32
    }
  }
}