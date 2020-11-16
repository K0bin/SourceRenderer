use legion::Entity;

pub struct Camera {
  pub fov: f32
}

pub struct ActiveCamera(pub Entity);
