use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct BoneController {
    pub bone: i32,
    pub bone_controller_type: i32,
    pub start: f32,
    pub end: f32,
    pub rest: i32,
    pub input_field: i32,
}

impl BoneController {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let bone = read.read_i32()?;
        let bone_controller_type = read.read_i32()?;
        let start = read.read_f32()?;
        let end = read.read_f32()?;
        let rest = read.read_i32()?;
        let input_field = read.read_i32()?;
        for _ in 0..8 {
            read.read_i32()?;
        }

        Ok(Self {
            bone,
            bone_controller_type,
            start,
            end,
            rest,
            input_field,
        })
    }
}
