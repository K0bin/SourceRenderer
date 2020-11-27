use std::io::{Read, Result as IOResult};
use lump_data::{LumpData, LumpType};
use ::{read_i16, read_u16};

pub struct BrushSide {
  pub plane_number: u16,
  pub texture_info: i16,
  pub displacement_info: i16,
  pub is_bevel_plane: bool
}

impl LumpData for BrushSide {
  fn lump_type() -> LumpType {
    LumpType::BrushSides
  }

  fn element_size(_version: i32) -> usize {
    8
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let plane_number = read_u16(reader)?;
    let texture_info = read_i16(reader)?;
    let displacement_info = read_i16(reader)?;
    let is_bevel_plane = read_i16(reader)? != 0;
    return Ok(Self {
      plane_number,
      texture_info,
      displacement_info,
      is_bevel_plane
    });
  }
}
