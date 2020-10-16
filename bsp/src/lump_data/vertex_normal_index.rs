use std::io::{Read, Result as IOResult};
use lump_data::{LumpData, LumpType};
use read_u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct VertexNormalIndex {
  pub index: u32
}

impl LumpData for VertexNormalIndex {
  fn lump_type() -> LumpType {
    LumpType::VertexNormalIndices
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let index = read_u32(reader)?;
    return Ok(Self {
      index
    });
  }
}
