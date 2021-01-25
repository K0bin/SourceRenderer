use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4, Vector2};

use crate::{PrimitiveRead, BoneWeight};

pub struct Vertex {
  pub bone_weights: BoneWeight,
  pub vec_position: Vector3<f32>,
  pub vec_normal: Vector3<f32>,
  pub vec_tex_coord: Vector2<f32>,
}

impl Vertex {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
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
