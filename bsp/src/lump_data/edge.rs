use std::io::{Read, Result};
use byteorder::{ReadBytesExt, LittleEndian};

pub const EDGE_SIZE: u8 = 4;

#[derive(Copy, Clone, Debug, Default)]
pub struct Edge {
  pub vertex_index: [u16; 2]
}

impl Edge {
  pub fn read(reader: &mut dyn Read) -> Result<Self> {
    let vertex_index = [
      reader.read_u16::<LittleEndian>()?,
      reader.read_u16::<LittleEndian>()?
    ];
    return Ok(Self {
      vertex_index
    });
  }
}
