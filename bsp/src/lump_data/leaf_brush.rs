use std::io::{Read, Result as IOResult};
use lump_data::{LumpData, LumpType};
use read_u16;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LeafBrush {
  pub index: u16
}

impl LumpData for LeafBrush {
  fn lump_type() -> LumpType {
    LumpType::LeafBrushes
  }

  fn element_size(_version: i32) -> usize {
    2
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let brush = read_u16(reader)?;
    return Ok(Self {
      index: brush
    });
  }
}
