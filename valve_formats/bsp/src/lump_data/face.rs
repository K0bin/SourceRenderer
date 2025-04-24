use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use std::io::{Read, Result as IOResult};

pub struct Face {
    pub plane_index: u16,
    pub size: u8,
    pub is_on_node: bool, // u8 in struct
    pub first_edge: i32,
    pub edges_count: i16,
    pub texture_info: i16,
    pub displacement_info: i16,
    pub surface_fog_volume_id: i16,
    pub styles: [u8; 4],
    pub light_offset: i32,
    pub area: f32,
    pub lightmap_texture_mins_in_luxels: [i32; 2],
    pub lightmap_texture_size_in_luxels: [i32; 2],
    pub original_face: i32,
    pub primitives_count: u16,
    pub first_primitive_id: u16,
    pub smoothing_group: u32,
}

impl LumpData for Face {
    fn lump_type() -> LumpType {
        LumpType::Faces
    }
    fn lump_type_hdr() -> Option<LumpType> {
        Some(LumpType::FacesHDR)
    }

    fn element_size(_version: i32) -> usize {
        56
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let plane_number = reader.read_u16()?;
        let size = reader.read_u8()?;
        let is_on_node = reader.read_u8()? != 0;
        let first_edge = reader.read_i32()?;
        let edges_count = reader.read_i16()?;
        let texture_info = reader.read_i16()?;
        let displacement_info = reader.read_i16()?;
        let surface_fog_volume_id = reader.read_i16()?;
        let styles = [
            reader.read_u8()?,
            reader.read_u8()?,
            reader.read_u8()?,
            reader.read_u8()?,
        ];
        let light_offset = reader.read_i32()?;
        let area = reader.read_f32()?;
        let lightmap_texture_mins_in_luxels = [reader.read_i32()?, reader.read_i32()?];
        let lightmap_texture_size_in_luxels = [reader.read_i32()?, reader.read_i32()?];
        let original_face = reader.read_i32()?;
        let primitives_count = reader.read_u16()?;
        let first_primitive_id = reader.read_u16()?;
        let smoothing_group = reader.read_u32()?;
        Ok(Self {
            plane_index: plane_number,
            size,
            is_on_node,
            first_edge,
            edges_count,
            texture_info,
            displacement_info,
            surface_fog_volume_id,
            styles,
            light_offset,
            area,
            lightmap_texture_mins_in_luxels,
            lightmap_texture_size_in_luxels,
            original_face,
            primitives_count,
            first_primitive_id,
            smoothing_group,
        })
    }
}
