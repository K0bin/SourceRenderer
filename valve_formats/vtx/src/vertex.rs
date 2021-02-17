use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct Vertex {
  pub bone_weight_index: [u8; 3],
  pub bones_count: u8,

  pub orig_mesh_vert_id: u16,

  pub bone_id: [i8; 3]
}

impl Vertex {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let bone_weight_index = [
      read.read_u8()?,
      read.read_u8()?,
      read.read_u8()?
    ];
    let bones_count = read.read_u8()?;
    let orig_mesh_vert_id = read.read_u16()?;
    let bone_id = [
      read.read_i8()?,
      read.read_i8()?,
      read.read_i8()?
    ];
    Ok(Self {
      bone_weight_index,
      bones_count,
      orig_mesh_vert_id,
      bone_id
    })
  }
}
