use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::Vector3;

use crate::PrimitiveRead;

pub struct Header2 {
  pub src_bone_transform_count: i32,
  pub src_bone_transform_index: i32,

  pub illum_position_attachment_index: i32,

  pub fl_max_eye_deflection: f32,

  pub linear_bone_index: i32
}

impl Header2 {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let src_bone_transform_count = read.read_i32()?;
    let src_bone_transform_index = read.read_i32()?;

    let illum_position_attachment_index = read.read_i32()?;

    let fl_max_eye_deflection = read.read_f32()?;

    let linear_bone_index = read.read_i32()?;

    Ok(Self {
      src_bone_transform_count,
      src_bone_transform_index,
      illum_position_attachment_index,
      fl_max_eye_deflection,
      linear_bone_index
    })
  }
}