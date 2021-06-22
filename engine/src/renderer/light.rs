use sourcerenderer_core::Vec3;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PointLight {
  pub position: Vec3,
  pub intensity: f32
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CullingPointLight {
  pub position: Vec3,
  pub radius: f32
}
