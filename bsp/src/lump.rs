use std::io::{Read, Result as IOResult};
use read_i32;

#[derive(Copy, Clone, Debug, Default)]
pub struct Lump {
  pub file_offset: i32,
  pub file_length: i32,
  pub version: i32,
  pub four_cc: i32,
}

impl Lump {
  pub fn read(reader: &mut dyn Read) -> IOResult<Self> {
    let file_offset = read_i32(reader)?;
    let file_length = read_i32(reader)?;
    let version = read_i32(reader)?;
    let four_cc = read_i32(reader)?;

    return Ok(Self {
      file_offset,
      file_length,
      version,
      four_cc,
    });
  }
}
