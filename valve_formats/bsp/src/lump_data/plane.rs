use std::io::{Read, Result as IOResult};
use nalgebra::Vector3;
use lump_data::{LumpData, LumpType};
use ::{read_f32, read_i32};

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
    let normal = Vector3::<f32>::new(read_f32(reader)?, read_f32(reader)?, read_f32(reader)?);
    let dist = read_f32(reader)?;
    let edge_type = read_i32(reader)?;
    return Ok(Self {
      normal,
      dist,
      edge_type
    });
  }
}
