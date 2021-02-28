use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use nalgebra::Vector3;

pub struct BrushModel {
  pub min: Vector3<f32>,
  pub max: Vector3<f32>,
  pub first_face: i32,
  pub num_faces: i32,
  pub brush: i32,
  pub brush_count: i32
}

impl LumpData for BrushModel {
  fn lump_type() -> LumpType {
    LumpType::Models
  }

  fn element_size(_version: i32) -> usize {
    40
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let min = Vector3::<f32>::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let max = Vector3::<f32>::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let first_face = reader.read_i32()?;
    let num_faces = reader.read_i32()?;
    let brush = reader.read_i32()?;
    let brush_count = reader.read_i32()?;
    Ok(Self {
      min,
      max,
      first_face,
      num_faces,
      brush,
      brush_count
    })
  }
}
