use crate::RawDataRead;
use std::ffi::CStr;
use std::io::{Read, Result as IOResult};

pub struct TextureStringData {
    pub data: Box<[u8]>,
}

impl TextureStringData {
    pub fn read(read: &mut dyn Read, length: u32) -> IOResult<Self> {
        let data = read.read_data(length as usize)?;
        Ok(Self { data })
    }

    pub fn get_string_at(&self, offset: u32) -> &CStr {
        let offset_data = &self.data[offset as usize..];
        let mut counter = 0usize;
        for char in offset_data {
            counter += 1;
            if *char == 0 {
                break;
            }
        }
        return unsafe { CStr::from_bytes_with_nul_unchecked(&offset_data[..counter as usize]) };
    }
}
