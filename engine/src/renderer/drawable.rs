use sourcerenderer_core::Matrix4;

use legion::Entity;
use std::{sync::Arc, usize};
use std::f32;
use sourcerenderer_core::graphics::Backend;
use crate::renderer::renderer_assets::*;

pub(super) struct RendererStaticDrawable<B: Backend> {
  pub(super) entity: Entity,
  pub(super) transform: Matrix4,
  pub(super) old_transform: Matrix4,
  pub(super) model: Arc<RendererModel<B>>,
  pub(super) receive_shadows: bool,
  pub(super) cast_shadows: bool,
  pub(super) can_move: bool
}

#[derive(Clone)]
pub(crate) struct View {
  pub(super) view_matrix: Matrix4,
  pub(super) proj_matrix: Matrix4,
  pub(super) old_camera_matrix: Matrix4,
  pub(super) camera_transform: Matrix4,
  pub(super) camera_fov: f32,
  pub(super) near_plane: f32,
  pub(super) far_plane: f32,
  pub(super) drawable_parts: Vec<DrawablePart>
}

impl Default for View {
  fn default() -> Self {
    Self {
      camera_transform: Matrix4::identity(),
      old_camera_matrix: Matrix4::identity(),
      view_matrix: Matrix4::identity(),
      proj_matrix: Matrix4::identity(),
      camera_fov: f32::consts::PI / 2f32,
      near_plane: 0.1f32,
      far_plane: 100f32,
      drawable_parts: Vec::new()
    }
  }
}

#[derive(Clone)]
pub struct DrawablePart {
  pub(super) drawable_index: usize,
  pub(super) part_index: usize
}
