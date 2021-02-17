use std::io::{Read, Result as IOResult};

use nalgebra::Vector4;

use crate::PrimitiveRead;

pub struct Tangent {
  pub data: Vector4::<f32>
}

impl Tangent {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let data = Vector4::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?);
    Ok(Self {
      data
    })
  }
}
