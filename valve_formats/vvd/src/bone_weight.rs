use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct BoneWeight {
  pub weight: [f32; 3],
  pub bone: [i8; 3],
  pub bones_count: u8,
}

impl BoneWeight {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let weight = [ read.read_f32()?, read.read_f32()?, read.read_f32()? ];
    let bone = [ read.read_i8()?, read.read_i8()?, read.read_i8()? ];
    let bones_count = read.read_u8()?;
    Ok(Self {
      weight,
      bone,
      bones_count
    })
  }
}
