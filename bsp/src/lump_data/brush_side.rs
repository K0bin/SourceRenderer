use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::{LumpData, LumpType};

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
    let plane_number = reader.read_u16::<LittleEndian>()?;
    let texture_info = reader.read_i16::<LittleEndian>()?;
    let displacement_info = reader.read_i16::<LittleEndian>()?;
    let is_bevel_plane = reader.read_i16::<LittleEndian>()? != 0;
    return Ok(Self {
      plane_number,
      texture_info,
      displacement_info,
      is_bevel_plane
    });
  }
}
