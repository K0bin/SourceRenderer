use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct Header {
  pub id: i32,
  pub version: i32,
  pub checksum: i32,
  pub lods_count: i32,
  pub lod_vertexes_count: [i32; 8],
  pub fixups_count: i32,
  pub fixup_table_start: i32,
  pub vertex_data_start: i32,
  pub tangent_data_start: i32
}

impl Header {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let id = read.read_i32()?;
    let version = read.read_i32()?;
    let checksum = read.read_i32()?;
    let lods_count = read.read_i32()?;
    let mut lod_vertexes_count = [0i32; 8];
    for i in 0..8 {
      lod_vertexes_count[i] = read.read_i32()?;
    }
    let fixups_count = read.read_i32()?;
    let fixup_table_start = read.read_i32()?;
    let vertex_data_start = read.read_i32()?;
    let tangent_data_start = read.read_i32()?;

    Ok(Self {
      id,
      version,
      checksum,
      lods_count,
      lod_vertexes_count,
      fixups_count,
      fixup_table_start,
      vertex_data_start,
      tangent_data_start
    })
  }
}
