use std::io::{Read, Result as IOResult};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::{LumpData, LumpType};

pub struct Face {
  pub plane_number: u16,
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
    let plane_number = reader.read_u16::<LittleEndian>()?;
    let size = reader.read_u8()?;
    let is_on_node = reader.read_u8()? != 0;
    let first_edge = reader.read_i32::<LittleEndian>()?;
    let edges_count = reader.read_i16::<LittleEndian>()?;
    let texture_info = reader.read_i16::<LittleEndian>()?;
    let displacement_info = reader.read_i16::<LittleEndian>()?;
    let surface_fog_volume_id = reader.read_i16::<LittleEndian>()?;
    let styles = [
      reader.read_u8()?,
      reader.read_u8()?,
      reader.read_u8()?,
      reader.read_u8()?
    ];
    let light_offset = reader.read_i32::<LittleEndian>()?;
    let area = reader.read_f32::<LittleEndian>()?;
    let lightmap_texture_mins_in_luxels = [reader.read_i32::<LittleEndian>()?, reader.read_i32::<LittleEndian>()?];
    let lightmap_texture_size_in_luxels = [reader.read_i32::<LittleEndian>()?, reader.read_i32::<LittleEndian>()?];
    let original_face = reader.read_i32::<LittleEndian>()?;
    let primitives_count = reader.read_u16::<LittleEndian>()?;
    let first_primitive_id = reader.read_u16::<LittleEndian>()?;
    let smoothing_group = reader.read_u32::<LittleEndian>()?;
    return Ok(Self {
      plane_number,
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
