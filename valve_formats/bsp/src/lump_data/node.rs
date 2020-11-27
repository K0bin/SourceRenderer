use std::io::{Read, Result as IOResult};
use lump_data::{LumpData, LumpType};
use ::{read_i32, read_i16};
use read_u16;

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

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let plane_number = read_i32(reader)?;
    let children: [i32; 2] = [
      read_i32(reader)?,
      read_i32(reader)?
    ];

    let mins: [i16; 3] = [
      read_i16(reader)?,
      read_i16(reader)?,
      read_i16(reader)?
    ];

    let maxs: [i16; 3] = [
      read_i16(reader)?,
      read_i16(reader)?,
      read_i16(reader)?
    ];

    let first_face = read_u16(reader)?;
    let faces_count = read_u16(reader)?;
    let area = read_u16(reader)?;
    let padding = read_u16(reader)?;

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
