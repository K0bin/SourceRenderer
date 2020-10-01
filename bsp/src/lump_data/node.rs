use std::io::{Read, Result};
use byteorder::{ReadBytesExt, LittleEndian};

pub const NODE_SIZE: u8 = 32;

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

impl Node {
  pub fn read(reader: &mut dyn Read) -> Result<Self> {
    let plane_number = reader.read_i32::<LittleEndian>()?;
    let children: [i32; 2] = [
      reader.read_i32::<LittleEndian>()?,
      reader.read_i32::<LittleEndian>()?
    ];

    let mins: [i16; 3] = [
      reader.read_i16::<LittleEndian>()?,
      reader.read_i16::<LittleEndian>()?,
      reader.read_i16::<LittleEndian>()?
    ];

    let maxs: [i16; 3] = [
      reader.read_i16::<LittleEndian>()?,
      reader.read_i16::<LittleEndian>()?,
      reader.read_i16::<LittleEndian>()?
    ];

    let first_face = reader.read_u16::<LittleEndian>()?;
    let faces_count = reader.read_u16::<LittleEndian>()?;
    let area = reader.read_u16::<LittleEndian>()?;
    let padding = reader.read_u16::<LittleEndian>()?;

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
