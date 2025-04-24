use bevy_math::Vec3;
use std::io::{Read, Result as IOResult};

use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Clone, Debug)]
pub struct VertexNormal {
    pub normal: Vec3,
}

impl LumpData for VertexNormal {
    fn lump_type() -> LumpType {
        LumpType::VertexNormals
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        12
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let vec3 = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        Ok(Self { normal: vec3 })
    }
}
