use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::Vector3;

use crate::PrimitiveRead;

pub struct Texture {
  pub name_offset: i32,
  pub flags: i32,
  pub used: i32,
  pub material: i32,
  pub client_material: i32
}

impl Texture {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let name_offset = read.read_i32()?;
    let flags = read.read_i32()?;
    let used = read.read_i32()?;
    let _unused = read.read_i32()?;
    let material = read.read_i32()?;
    let client_material = read.read_i32()?;

    let mut _unused1 = [0u8; 10 * 4];
    read.read_exact(&mut _unused1)?;

    Ok(Self {
      name_offset,
      flags,
      used,
      material,
      client_material
    })
  }
}
