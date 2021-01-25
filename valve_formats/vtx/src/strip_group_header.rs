use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct StripGroupHeader {
  pub verts_count: i32,
  pub vert_offset: i32,

  pub indices_count: i32,
  pub indices_offset: i32,

  pub strips_count: i32,
  pub strips_offset: i32,

  pub flags: u8
}

impl StripGroupHeader {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let verts_count = read.read_i32()?;
    let vert_offset = read.read_i32()?;
    let indices_count = read.read_i32()?;
    let indices_offset = read.read_i32()?;
    let strips_count = read.read_i32()?;
    let strips_offset = read.read_i32()?;
    let flags = read.read_u8()?;
    let _padding = read.read_u8()?; // maybe wrong?
    Ok(Self {
      vert_offset,
      verts_count,
      indices_count,
      indices_offset,
      strips_count,
      strips_offset,
      flags
    })
  }
}
