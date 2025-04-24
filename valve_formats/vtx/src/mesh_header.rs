use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

bitflags! {
  pub struct MeshFlags : u8 {
    const STRIPGROUP_IS_FLEXED = 0x01;
    const STRIPGROUP_IS_HWSKINNED = 0x02;
    const STRIPGROUP_IS_DELTA_FLEXED = 0x04;
    const STRIPGROUP_SUPPRESS_HW_MORPH = 0x08;
  }
}

pub struct MeshHeader {
    pub strip_groups_count: i32,
    pub strip_group_header_offset: i32,

    pub flags: MeshFlags,
}

impl MeshHeader {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let strip_groups_count = read.read_i32()?;
        let strip_group_header_offset = read.read_i32()?;
        let flags_raw = read.read_u8()?;
        let flags = MeshFlags::from_bits(flags_raw).unwrap_or(MeshFlags::empty());
        Ok(Self {
            strip_groups_count,
            strip_group_header_offset,
            flags,
        })
    }
}
