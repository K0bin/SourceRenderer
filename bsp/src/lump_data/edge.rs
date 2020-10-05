use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::{LumpData, LumpType};

#[derive(Copy, Clone, Debug, Default)]
pub struct Edge {
  pub vertex_index: [u16; 2]
}

impl LumpData for Edge {
  fn lump_type() -> LumpType {
    LumpType::Edges
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let vertex_index = [
      reader.read_u16::<LittleEndian>()?,
      reader.read_u16::<LittleEndian>()?
    ];
    return Ok(Self {
      vertex_index
    });
  }
}
