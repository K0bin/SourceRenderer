use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use crate::transform::GlobalTransform;
use legion::systems::{CommandBuffer, System, Builder};
use legion::{Entity, maybe_changed};
use nalgebra::{Point3, Transform3, Translation3, Isometry3, Unit};

pub struct Camera {
  pub fov: f32
}

pub struct ActiveCamera(pub Entity);
