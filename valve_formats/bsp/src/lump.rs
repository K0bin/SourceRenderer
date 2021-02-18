use std::io::{Read, Result as IOResult};
use crate::PrimitiveRead;

#[derive(Copy, Clone, Debug, Default)]
pub struct Lump {
  pub file_offset: i32,
  pub file_length: i32,
  pub version: i32,
  pub four_cc: i32,
}

impl Lump {
  pub fn read(reader: &mut dyn Read) -> IOResult<Self> {
    let file_offset = reader.read_i32()?;
    let file_length = reader.read_i32()?;
    let version = reader.read_i32()?;
    let four_cc = reader.read_i32()?;

    Ok(Self {
      file_offset,
      file_length,
      version,
      four_cc,
    })
  }
}
