use bevy_math::Vec3;
use std::io::{Read, Result as IOResult};

use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

pub struct BrushModel {
    pub min: Vec3,
    pub max: Vec3,
    pub origin: Vec3,
    pub head_node: i32,
    pub first_face: i32,
    pub num_faces: i32,
}

impl LumpData for BrushModel {
    fn lump_type() -> LumpType {
        LumpType::Models
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        48
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let min = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let max = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let origin = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let head_node = reader.read_i32()?;
        let first_face = reader.read_i32()?;
        let num_faces = reader.read_i32()?;
        Ok(Self {
            min,
            max,
            origin,
            head_node,
            first_face,
            num_faces,
        })
    }
}
