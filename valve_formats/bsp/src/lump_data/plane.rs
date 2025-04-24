use bevy_math::Vec3;
use std::io::{Read, Result as IOResult};

use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Copy, Clone, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub dist: f32,
    pub edge_type: i32,
}

impl LumpData for Plane {
    fn lump_type() -> LumpType {
        LumpType::Planes
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        20
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let normal = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let dist = reader.read_f32()?;
        let edge_type = reader.read_i32()?;
        Ok(Self {
            normal,
            dist,
            edge_type,
        })
    }
}
