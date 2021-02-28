use crate::lump::Lump;

use std::io::{Read, Result as IOResult};
use crate::PrimitiveRead;

const LUMP_COUNT: usize = 16;

pub struct MapHeader {
  pub identifier: i32,
  pub version: i32,
  pub lumps: [Lump; LUMP_COUNT],
}

impl MapHeader {
  pub fn read(reader: &mut dyn Read) -> IOResult<MapHeader> {
    let identifier = reader.read_i32()?;
    let version = reader.read_i32()?;
    let mut lumps: [Lump; LUMP_COUNT] = [
      Lump {
        file_offset: 0,
        file_length: 0
      };
      LUMP_COUNT
    ];
    for i in 0..LUMP_COUNT {
      let lump = Lump::read(reader)?;
      lumps[i] = lump;
    }
    Ok(MapHeader {
      identifier,
      version,
      lumps,
    })
  }
}
