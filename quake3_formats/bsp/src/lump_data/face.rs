use std::io::{Read, Result as IOResult, Error as IOError, ErrorKind};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

pub enum FaceType {
  Polygon = 1,
  Patch = 2,
  Mesh = 3,
  Billboard = 4
}

pub struct Face {
  pub texture: i32,
  pub effect: i32,
  pub face_type: FaceType,
  pub vertex_count: i32,
  pub mesh_vert: i32,
  pub mesh_vert_count: i32,
  pub lightmap_index: i32,
  pub lightmap_start: [i32; 2],
  pub lightmap_size: [i32; 2],
  pub lightmap_origin: [f32; 3],
  pub lightmap_vecs: [f32; 3],
  pub size: [i32; 2]
}

impl LumpData for Face {
  fn lump_type() -> LumpType {
    LumpType::Faces
  }

  fn element_size(_version: i32) -> usize {
    72
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let texture = reader.read_i32()?;
    let effect = reader.read_i32()?;
    let face_type = match reader.read_i32()? {
      1 => FaceType::Polygon,
      2 => FaceType::Patch,
      3 => FaceType::Mesh,
      4 => FaceType::Billboard,
      _ =>  {
        return Err(IOError::new(ErrorKind::Other, "Invalid face type"));
      }
    };
    let vertex_count = reader.read_i32()?;
    let mesh_vert = reader.read_i32()?;
    let mesh_vert_count = reader.read_i32()?;
    let lightmap_index = reader.read_i32()?;
    let lightmap_start = [reader.read_i32()?, reader.read_i32()?];
    let lightmap_size = [reader.read_i32()?, reader.read_i32()?];
    let lightmap_origin = [reader.read_f32()?, reader.read_f32()?, reader.read_f32()?];
    let lightmap_vecs = [reader.read_f32()?, reader.read_f32()?, reader.read_f32()?];
    let size = [reader.read_i32()?, reader.read_i32()?];
    Ok(Self {
      texture,
      effect,
      face_type,
      vertex_count,
      mesh_vert,
      mesh_vert_count,
      lightmap_index,
      lightmap_start,
      lightmap_size,
      lightmap_origin,
      lightmap_vecs,
      size
    })
  }
}
