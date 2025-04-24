use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use bevy_math::Vec3;
use std::io::{Read, Result as IOResult};

#[derive(Clone, Debug)]
pub struct Vertex {
    pub position: Vec3,
}

impl LumpData for Vertex {
    fn lump_type() -> LumpType {
        LumpType::Vertices
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        12
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let vec3 = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        Ok(Self { position: vec3 })
    }
}
