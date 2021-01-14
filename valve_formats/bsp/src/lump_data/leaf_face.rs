use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LeafFace {
  pub index: u16
}

impl LumpData for LeafFace {
  fn lump_type() -> LumpType {
    LumpType::LeafFaces
  }
  fn lump_type_hdr() -> Option<LumpType> {
    None
  }

  fn element_size(_version: i32) -> usize {
    2
  }

  fn read(mut reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let face = reader.read_u16()?;
    return Ok(Self {
      index: face
    });
  }
}
