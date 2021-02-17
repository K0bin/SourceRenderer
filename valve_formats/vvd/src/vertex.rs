use std::io::{Read, Result as IOResult};

use nalgebra::{Vector2, Vector3};

use crate::{PrimitiveRead, BoneWeight};

pub struct Vertex {
  pub bone_weights: BoneWeight,
  pub vec_position: Vector3<f32>,
  pub vec_normal: Vector3<f32>,
  pub vec_tex_coord: Vector2<f32>,
}

impl Vertex {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let bone_weights = BoneWeight::read(read)?;
    let vec_position = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let vec_normal = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let vec_tex_coord = Vector2::<f32>::new(read.read_f32()?, read.read_f32()?);
    Ok(Self {
      bone_weights,
      vec_position,
      vec_normal,
      vec_tex_coord
    })
  }
}
