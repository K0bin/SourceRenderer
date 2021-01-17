use super::leaf::ColorRGBExp32;
use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[repr(C)]
pub struct Lighting {
  pub color: ColorRGBExp32
}

impl LumpData for Lighting {
  fn lump_type() -> LumpType {
    LumpType::Lighting
  }
  fn lump_type_hdr() -> Option<LumpType> {
    Some(LumpType::LightingHDR)
  }

  fn element_size(_version: i32) -> usize {
    4
  }

  fn read(read: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let color = ColorRGBExp32::read(read)?;
    Ok(Self {
      color
    })
  }
}
