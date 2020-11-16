use std::io::{Read, Result as IOResult};
use lump_data::brush::BrushContents;
use lump_data::{LumpData, LumpType};
use ::{read_u8, read_i8};
use ::{read_u16, read_i16};
use read_u32;

#[derive(Copy, Clone, Debug, Default)]
pub struct ColorRGBExp32 {
  pub r: u8,
  pub g: u8,
  pub b: u8,
  pub exponent: i8,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct CompressedLightCube {
  pub color: [ColorRGBExp32; 6]
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Leaf {
  pub contents: BrushContents,
  pub cluster: i16,
  pub area: i16,
  pub flags: i16,
  pub mins: [i16; 3],
  pub maxs: [i16; 3],
  pub first_leaf_face: u16,
  pub leaf_faces_count: u16,
  pub first_leaf_brush: u16,
  pub leaf_brushes_count: u16,
  pub leaf_water_data_id: i16,
  pub ambient_lighting: CompressedLightCube,
  pub padding: i16,
}

impl ColorRGBExp32 {
  fn read(reader: &mut dyn Read) -> IOResult<Self> {
    let r = read_u8(reader)?;
    let g = read_u8(reader)?;
    let b = read_u8(reader)?;
    let _padding = read_u8(reader);
    let exponent = read_i8(reader)?;
    return Ok(Self {
      r,
      g,
      b,
      exponent,
    });
  }
}

impl CompressedLightCube {
  fn read(reader: &mut dyn Read) -> IOResult<Self> {
    let mut colors: [ColorRGBExp32; 6] = [Default::default(); 6];
    for i in 0..6 {
      let color = ColorRGBExp32::read(reader)?;
      colors[i] = color;
    }
    return Ok(Self {
      color: colors
    });
  }
}

impl LumpData for Leaf {
  fn lump_type() -> LumpType {
    LumpType::Leafs
  }

  fn element_size(version: i32) -> usize {
    if version >= 19 {
      56
    } else {
      32
    }
  }

  fn read(reader: &mut dyn Read, version: i32) -> IOResult<Self> {
    let contents = read_u32(reader)?;
    let cluster = read_i16(reader)?;
    let area_flags = read_u16(reader)?;
    let area: i16 = ((area_flags & 0b1111_1111_1000_0000) >> 7) as i16;
    let flags: i16 = (area_flags & 0b0000_0000_0111_1111) as i16;

    let mins: [i16; 3] = [
      read_i16(reader)?,
      read_i16(reader)?,
      read_i16(reader)?
    ];

    let maxs: [i16; 3] = [
      read_i16(reader)?,
      read_i16(reader)?,
      read_i16(reader)?
    ];

    let first_leaf_face = read_u16(reader)?;
    let leaf_faces_count = read_u16(reader)?;
    let first_leaf_brush = read_u16(reader)?;
    let leaf_brushes_count = read_u16(reader)?;
    let leaf_water_data_id = read_i16(reader)?;
    let mut padding: i16 = 0;
    let mut ambient_lighting: CompressedLightCube = Default::default();
    if version <= 19 {
      let ambient_lighting_res = CompressedLightCube::read(reader)?;
      ambient_lighting = ambient_lighting_res;
      let padding_res = read_i16(reader)?;
      padding = padding_res;
    }

    return Ok(Self {
      contents: BrushContents::new(contents),
      cluster,
      area,
      flags,
      mins,
      maxs,
      first_leaf_face,
      leaf_faces_count,
      first_leaf_brush,
      leaf_brushes_count,
      leaf_water_data_id,
      ambient_lighting,
      padding,
    });
  }
}
