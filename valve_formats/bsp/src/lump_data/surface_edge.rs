use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct  SurfaceEdge {
  pub index: i32
}

impl LumpData for SurfaceEdge {
  fn lump_type() -> LumpType {
    LumpType::SurfaceEdges
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(mut reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let edge = reader.read_i32()?;
    return Ok(Self {
      index: edge
    });
  }
}
