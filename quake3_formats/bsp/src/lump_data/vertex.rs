use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use nalgebra::{Vector2, Vector3};
use std::io::{Read, Result as IOResult};

#[derive(Clone, Debug)]
pub struct Vertex {
    pub position: Vector3<f32>,
    pub tex_coord: [Vector2<f32>; 2],
    pub normal: Vector3<f32>,
    pub color: [u8; 4],
}

impl LumpData for Vertex {
    fn lump_type() -> LumpType {
        LumpType::Vertices
    }

    fn element_size(_version: i32) -> usize {
        44
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let position =
            Vector3::<f32>::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let tex_coord = [
            Vector2::<f32>::new(reader.read_f32()?, reader.read_f32()?),
            Vector2::<f32>::new(reader.read_f32()?, reader.read_f32()?),
        ];
        let normal =
            Vector3::<f32>::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);
        let color = [
            reader.read_u8()?,
            reader.read_u8()?,
            reader.read_u8()?,
            reader.read_u8()?,
        ];
        Ok(Self {
            position,
            tex_coord,
            normal,
            color,
        })
    }
}
