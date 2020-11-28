use crate::{LumpData, LumpType, RawDataRead};
use std::io::{Read, Result as IOResult};
use std::ffi::CStr;

pub struct TextureStringData {
  pub data: Box<[u8]>
}

impl TextureStringData {
  pub fn read(mut read: &mut dyn Read, length: u32) -> IOResult<Self> {
    let data = read.read_data(length as usize)?;
    Ok(Self {
      data
    })
  }

  pub fn get_string_at(&self, offset: u32) -> &CStr {
    let offset_data = &self.data[offset as usize .. ];
    let mut counter = 0usize;
    for char in offset_data {
      if *char == 0 {
        break;
      }
      counter += 1;
    }
    return unsafe { CStr::from_bytes_with_nul_unchecked(&offset_data[.. counter as usize]) };
  }
}
