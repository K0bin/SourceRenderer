use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct HitboxSet {
  pub name_index: i32,
  pub hitboxes_count: i32,
  pub hitboxes_index: i32
}

impl HitboxSet {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let name_index = read.read_i32()?;
    let hitboxes_count = read.read_i32()?;
    let hitboxes_index = read.read_i32()?;

    Ok(Self {
      name_index,
      hitboxes_count,
      hitboxes_index
    })
  }
}
