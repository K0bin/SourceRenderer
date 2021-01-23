use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::{PrimitiveRead, Model};

pub struct BodyPart {
  pub name_index: u64,
  pub models_count: i32,
  pub base: i32,
  pub model_index: u64
}

impl BodyPart {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let name_index = read.read_i32()?;
    let models_count = read.read_i32()?;
    let base = read.read_i32()?;
    let model_index = read.read_i32()?;

    Ok(Self {
      name_index: name_index as u64,
      models_count,
      base,
      model_index: model_index as u64
    })
  }
}
