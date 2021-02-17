use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct Header {
  pub version: i32,
  pub vert_cache_size: i32,
  pub max_bones_per_strip: u16,
  pub max_bones_per_tri: u16,
  pub max_bones_per_vert: i32,

  pub checksum: i32,

  pub lods_count: i32,

  pub material_replacement_list_offset: i32,

  pub body_parts_count: i32,
  pub body_parts_offset: i32
}

impl Header {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let version = read.read_i32()?;
    let vert_cache_size = read.read_i32()?;
    let max_bones_per_strip = read.read_u16()?;
    let max_bones_per_tri = read.read_u16()?;
    let max_bones_per_vert = read.read_i32()?;

    let checksum = read.read_i32()?;

    let lods_count = read.read_i32()?;

    let material_replacement_list_offset = read.read_i32()?;

    let body_parts_count = read.read_i32()?;
    let body_parts_offset = read.read_i32()?;

    Ok(Self {
      version,
      vert_cache_size,
      max_bones_per_strip,
      max_bones_per_tri,
      max_bones_per_vert,
      checksum,
      lods_count,
      material_replacement_list_offset,
      body_parts_count,
      body_parts_offset
    })
  }
}
