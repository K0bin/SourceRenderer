use std::io::{Read, Result};
use byteorder::{ReadBytesExt, LittleEndian};

pub const BRUSH_SIDE_SIZE: u8 = 8;

pub struct BrushSide {
  pub plane_number: u16,
  pub texture_info: i16,
  pub displacement_info: i16,
  pub is_bevel_plane: bool
}

impl BrushSide {
  pub fn read(reader: &mut dyn Read) -> Result<Self> {
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
