use std::io::{Read, Result as IOResult};
use lump_data::{LumpData, LumpType};
use ::read_f32;
use ::{read_u8, read_u16};
use ::{read_i16, read_i32};
use read_u32;

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
  pub smoothing_group: u32
}

impl LumpData for Face {
  fn lump_type() -> LumpType {
    LumpType::Faces
  }

  fn element_size(_version: i32) -> usize {
    56
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let plane_number = read_u16(reader)?;
    let size = read_u8(reader)?;
    let is_on_node = read_u8(reader)? != 0;
    let first_edge = read_i32(reader)?;
    let edges_count = read_i16(reader)?;
    let texture_info = read_i16(reader)?;
    let displacement_info = read_i16(reader)?;
    let surface_fog_volume_id = read_i16(reader)?;
    let styles = [
      read_u8(reader)?,
      read_u8(reader)?,
      read_u8(reader)?,
      read_u8(reader)?
    ];
    let light_offset = read_i32(reader)?;
    let area = read_f32(reader)?;
    let lightmap_texture_mins_in_luxels = [read_i32(reader)?, read_i32(reader)?];
    let lightmap_texture_size_in_luxels = [read_i32(reader)?, read_i32(reader)?];
    let original_face = read_i32(reader)?;
    let primitives_count = read_u16(reader)?;
    let first_primitive_id = read_u16(reader)?;
    let smoothing_group = read_u32(reader)?;
    return Ok(Self {
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
      smoothing_group
    });
  }
}
