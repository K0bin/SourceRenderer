use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use nalgebra::Vector3;
use lump_data::{LumpData, LumpType};

#[derive(Copy, Clone, Debug)]
pub struct Plane {
  pub normal: Vector3<f32>,
  pub dist: f32,
  pub edge_type: i32
}

impl LumpData for Plane {
  fn lump_type() -> LumpType {
    LumpType::Planes
  }

  fn element_size(_version: i32) -> usize {
    20
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let normal = Vector3::<f32>::new(reader.read_f32::<LittleEndian>()?, reader.read_f32::<LittleEndian>()?, reader.read_f32::<LittleEndian>()?);
    let dist = reader.read_f32::<LittleEndian>()?;
    let edge_type = reader.read_i32::<LittleEndian>()?;
    return Ok(Self {
      normal,
      dist,
      edge_type
    });
  }
}
