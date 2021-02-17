use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TextureDataStringTable(pub i32);

impl LumpData for TextureDataStringTable {
  fn lump_type() -> LumpType {
    LumpType::TextureDataStringTable
  }
  fn lump_type_hdr() -> Option<LumpType> {
    None
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let value = reader.read_i32()?;
    return Ok(Self(value));
  }
}
