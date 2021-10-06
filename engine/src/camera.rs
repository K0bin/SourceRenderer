use legion::Entity;

pub struct Camera {
  pub fov: f32,
  pub interpolate_rotation: bool
}

pub struct ActiveCamera(pub Entity);
