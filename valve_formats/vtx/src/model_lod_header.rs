use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct ModelLODHeader {
  pub meshes_count: i32,
  pub mesh_offset: i32
}

impl ModelLODHeader {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let meshes_count = read.read_i32()?;
    let meshes_offset = read.read_i32()?;
    Ok(Self {
      meshes_count,
      mesh_offset: meshes_offset
    })
  }
}
