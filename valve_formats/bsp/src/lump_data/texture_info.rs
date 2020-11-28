use nalgebra::Vector4;
use crate::{LumpData, LumpType, PrimitiveRead};
use std::io::{Read, Result as IOResult};

pub struct TextureInfo {
  pub texture_vecs_s: Vector4<f32>,
  pub texture_vecs_t: Vector4<f32>,
  pub lightmap_vecs_s: Vector4<f32>,
  pub lightmap_vecs_t: Vector4<f32>,
  pub flags: i32,
  pub texture_data: i32
}

impl LumpData for TextureInfo {
  fn lump_type() -> LumpType {
    LumpType::TextureInfo
  }

  fn element_size(_version: i32) -> usize {
    72
  }

  fn read(mut reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    return Ok(Self {
      texture_vecs_s: Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?),
      texture_vecs_t: Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?),
      lightmap_vecs_s: Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?),
      lightmap_vecs_t: Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?),
      flags: reader.read_i32()?,
      texture_data: reader.read_i32()?
    });
  }
}
