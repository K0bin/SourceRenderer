use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::{LumpData, LumpType};
use nalgebra::Vector3;

#[derive(Clone, Debug)]
pub struct VertexNormal {
  pub normal: Vector3<f32>
}

impl LumpData for VertexNormal {
  fn lump_type() -> LumpType {
    LumpType::VertexNormals
  }

  fn element_size(_version: i32) -> usize {
    12
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let vec3 = Vector3::<f32>::new(reader.read_f32::<LittleEndian>()?, reader.read_f32::<LittleEndian>()?, reader.read_f32::<LittleEndian>()?);
    return Ok(Self {
      normal: vec3
    });
  }
}
