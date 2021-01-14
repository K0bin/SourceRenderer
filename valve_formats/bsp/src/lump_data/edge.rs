use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Copy, Clone, Debug, Default)]
pub struct Edge {
  pub vertex_index: [u16; 2]
}

impl LumpData for Edge {
  fn lump_type() -> LumpType {
    LumpType::Edges
  }
  fn lump_type_hdr() -> Option<LumpType> {
    None
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(mut reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let vertex_index = [
      reader.read_u16()?,
      reader.read_u16()?
    ];
    return Ok(Self {
      vertex_index
    });
  }
}
