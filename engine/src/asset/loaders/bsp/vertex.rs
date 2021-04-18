use sourcerenderer_core::{Vec3, Vec2};

#[repr(C)]
#[derive(Clone, PartialEq, Debug)]
pub struct Vertex {
  pub position: Vec3,
  pub normal: Vec3,
  pub uv: Vec2,
  pub lightmap_uv: Vec2,
  pub alpha: f32
}
