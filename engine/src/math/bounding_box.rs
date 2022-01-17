use sourcerenderer_core::Vec3;

#[derive(Debug)]
pub struct BoundingBox {
  pub min: Vec3,
  pub max: Vec3
}

impl BoundingBox {
  pub fn new(min: Vec3, max: Vec3) -> Self {
    Self {
      min,
      max
    }
  }
}