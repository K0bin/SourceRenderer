use std::io::{Read, Result as IOResult};

use bevy_math::Vec4;

use crate::PrimitiveRead;

pub struct Tangent {
    pub data: Vec4,
}

impl Tangent {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let data = Vec4::new(
            read.read_f32()?,
            read.read_f32()?,
            read.read_f32()?,
            read.read_f32()?,
        );
        Ok(Self { data })
    }
}
