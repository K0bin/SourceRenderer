use sourcerenderer_core::{Vec3, Matrix4, Vec4};

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

  pub fn transform(&self, matrix: &Matrix4) -> BoundingBox {
    let a = matrix * Vec4::new(self.min.x, self.min.y, self.min.z, 1f32);
    let b = matrix * Vec4::new(self.max.x, self.max.y, self.max.z, 1f32);
    BoundingBox {
      min: Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
      max: Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
    }
  }

  pub fn contains(&self, point: &Vec3) -> bool {
    self.min.x <= point.x && point.x < self.max.x
    && self.min.y <= point.y && point.y < self.max.y
    && self.min.z <= point.z && point.z < self.max.z
  }
}