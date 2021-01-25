use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct ModelHeader {
  pub lods_count: i32,
  pub lod_offset: i32
}

impl ModelHeader {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let lods_count = read.read_i32()?;
    let lod_offset = read.read_i32()?;
    Ok(Self {
      lods_count,
      lod_offset
    })
  }
}
