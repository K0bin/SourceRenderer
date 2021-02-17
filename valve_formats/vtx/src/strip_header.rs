use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct StripHeader {
  pub indices_count: i32,
  pub index_offset: i32,

  pub verts_count: i32,
  pub vert_offset: i32,

  pub bones_count: i16,

  pub flags: u8,

  pub bone_state_changes_count: i32,
  pub bone_state_change_offset: i32
}

impl StripHeader {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let indices_count = read.read_i32()?;
    let index_offset = read.read_i32()?;
    let verts_count = read.read_i32()?;
    let vert_offset = read.read_i32()?;
    let bones_count = read.read_i16()?;
    let flags = read.read_u8()?;
    let bone_state_changes_count = read.read_i32()?;
    let bone_state_change_offset = read.read_i32()?;
    Ok(Self {
      indices_count,
      index_offset,
      verts_count,
      vert_offset,
      bones_count,
      flags,
      bone_state_changes_count,
      bone_state_change_offset
    })
  }
}
