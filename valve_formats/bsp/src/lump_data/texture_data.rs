use bevy_math::Vec3;

use crate::{LumpData, LumpType, PrimitiveRead};
use std::io::{Read, Result as IOResult};

pub struct TextureData {
    pub reflectivity: Vec3,
    pub name_string_table_id: i32,
    pub width: i32,
    pub height: i32,
    pub view_width: i32,
    pub view_height: i32,
}

impl LumpData for TextureData {
    fn lump_type() -> LumpType {
        LumpType::TextureData
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        32
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        Ok(Self {
            reflectivity: Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?),
            name_string_table_id: reader.read_i32()?,
            width: reader.read_i32()?,
            height: reader.read_i32()?,
            view_width: reader.read_i32()?,
            view_height: reader.read_i32()?,
        })
    }
}
