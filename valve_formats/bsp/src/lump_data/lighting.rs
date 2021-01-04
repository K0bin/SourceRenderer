use super::leaf::ColorRGBExp32;
use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

pub struct Lighting {
  pub color: ColorRGBExp32
}

impl LumpData for Lighting {
  fn lump_type() -> LumpType {
    LumpType::Lighting
  }

  fn element_size(version: i32) -> usize {
    5
  }

  fn read(read: &mut dyn Read, version: i32) -> IOResult<Self> {
    let color = ColorRGBExp32::read(read)?;
    Ok(Self {
      color
    })
  }
}
