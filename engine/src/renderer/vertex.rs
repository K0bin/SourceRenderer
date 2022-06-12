use sourcerenderer_core::{Vec2, Vec3};

#[repr(C)]
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Vertex {
  pub position: Vec3,
  pub _padding: u32,
  pub normal: Vec3,
  pub _padding1: u32,
  pub uv: Vec2,
  pub lightmap_uv: Vec2,
  pub alpha: f32,
  pub _padding2: u32,
  pub _padding3: u32,
  pub _padding4: u32,
}
