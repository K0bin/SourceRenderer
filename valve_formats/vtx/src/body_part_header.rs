use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct BodyPartHeader {
  pub models_count: i32,
  pub model_offset: i32
}

impl BodyPartHeader {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let models_count = read.read_i32()?;
    let model_offset = read.read_i32()?;
    Ok(Self {
      model_offset,
      models_count
    })
  }
}
