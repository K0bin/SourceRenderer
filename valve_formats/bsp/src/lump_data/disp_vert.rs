use bevy_math::Vec3;

use crate::{LumpType, LumpData};
use crate::PrimitiveRead;
use std::io::{Read, Result as IOResult};

pub struct DispVert {
  pub vec: Vec3,
  pub dist: f32,
  pub alpha: f32
}

impl LumpData for DispVert {
  fn lump_type() -> LumpType {
    LumpType::DisplacementVertices
  }
  fn lump_type_hdr() -> Option<LumpType> {
    None
  }

  fn element_size(_version: i32) -> usize {
    20
  }

  fn read(read: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let vec = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let dist = read.read_f32()?;
    let alpha = read.read_f32()?;
    Ok(Self {
      vec,
      dist,
      alpha
    })
  }
}
