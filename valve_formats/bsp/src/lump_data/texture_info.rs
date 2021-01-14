use nalgebra::Vector4;
use crate::{LumpData, LumpType, PrimitiveRead};
use std::io::{Read, Result as IOResult};

bitflags! {
  pub struct SurfaceFlags: i32 {
    const LIGHT = 0x1;
    const SKY2D = 0x2;
    const SKY = 0x4;
    const WARP = 0x8;
    const TRANS = 0x10;
    const NOPORTAL = 0x20;
    const TRIGGER = 0x40;
    const NODRAW = 0x80;
    const HINT = 0x100;
    const SKIP = 0x200;
    const NOLIGHT = 0x400;
    const BUMPLIGHT = 0x800;
    const NOSHADOWS = 0x1000;
    const NODECALS = 0x2000;
    const NOCHOP = 0x4000;
    const HITBOX = 0x8000;
  }
}

pub struct TextureInfo {
  pub texture_vecs_s: Vector4<f32>,
  pub texture_vecs_t: Vector4<f32>,
  pub lightmap_vecs_s: Vector4<f32>,
  pub lightmap_vecs_t: Vector4<f32>,
  pub flags: SurfaceFlags,
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
    let texture_vecs_s = Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let texture_vecs_t = Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let lightmap_vecs_s = Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let lightmap_vecs_t = Vector4::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
    let flags_bits = reader.read_i32()?;
    let flags = SurfaceFlags::from_bits(flags_bits).unwrap();
    let texture_data = reader.read_i32()?;

    return Ok(Self {
      texture_vecs_s,
      texture_vecs_t,
      lightmap_vecs_s,
      lightmap_vecs_t,
      flags,
      texture_data
    });
  }
}
