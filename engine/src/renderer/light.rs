use sourcerenderer_core::Vec3;

pub struct PointLight {
  pub position: Vec3,
  pub intensity: f32
}

pub struct CullingPointLight {
  pub position: Vec3,
  pub radius: f32
}
