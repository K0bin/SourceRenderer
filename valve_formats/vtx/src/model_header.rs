use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct ModelHeader {
    pub lods_count: i32,
    pub lod_offset: i32,
}

impl ModelHeader {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let lods_count = read.read_i32()?;
        let lod_offset = read.read_i32()?;
        Ok(Self {
            lods_count,
            lod_offset,
        })
    }
}
