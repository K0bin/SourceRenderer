use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveReader;

#[derive(Copy, Clone, Debug, Default)]
pub struct Node {
  pub plane_number: i32,
  pub children: [i32; 2],
  pub mins: [i16; 3],
  pub maxs: [i16; 3],
  pub first_face: u16,
  pub faces_count: u16,
  pub area: u16,
  pub padding: u16,
}

impl LumpData for Node {
  fn lump_type() -> LumpType {
    LumpType::Nodes
  }

  fn element_size(_version: i32) -> usize {
    32
  }

  fn read(mut reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let plane_number = reader.read_i32()?;
    let children: [i32; 2] = [
      reader.read_i32()?,
      reader.read_i32()?
    ];

    let mins: [i16; 3] = [
      reader.read_i16()?,
      reader.read_i16()?,
      reader.read_i16()?
    ];

    let maxs: [i16; 3] = [
      reader.read_i16()?,
      reader.read_i16()?,
      reader.read_i16()?
    ];

    let first_face = reader.read_u16()?;
    let faces_count = reader.read_u16()?;
    let area = reader.read_u16()?;
    let padding = reader.read_u16()?;

    return Ok(Self {
      plane_number,
      children,
      mins,
      maxs,
      first_face,
      faces_count,
      area,
      padding,
    });
  }
}
