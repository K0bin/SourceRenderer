use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::{LumpData, LumpType};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LeafFace {
  pub index: u16
}

impl LumpData for LeafFace {
  fn lump_type() -> LumpType {
    LumpType::LeafFaces
  }

  fn element_size(_version: i32) -> usize {
    2
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let face = reader.read_u16::<LittleEndian>()?;
    return Ok(Self {
      index: face
    });
  }
}
