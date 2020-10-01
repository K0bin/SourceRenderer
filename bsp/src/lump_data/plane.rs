use std::io::{Read, Result};
use byteorder::{ReadBytesExt, LittleEndian};
use nalgebra::Vector3;

pub const PLANE_SIZE: u8 = 20;

#[derive(Copy, Clone, Debug)]
pub struct Plane {
  pub normal: Vector3<f32>,
  pub dist: f32,
  pub edge_type: i32
}

impl Plane {
  pub fn read(reader: &mut dyn Read) -> Result<Self> {
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
