use std::io::{Read, Result as IOResult};

use crate::{PrimitiveRead, StringRead};

pub struct Model {
  pub name: String,
  pub model_type: i32,
  pub bounding_radius: f32,
  pub meshes_count: i32,
  pub mesh_index: u64,

  pub vertices_count: i32,
  pub vertex_index: i32,
  pub tangents_index: i32,

  pub attachments_count: i32,
  pub attachment_index: i32,

  pub eye_balls_count: i32,
  pub eye_ball_index: i32,

  pub vertex_data : ModelVertexData
}

pub struct ModelVertexData {
  _unknown: [u8; 8]
}

impl ModelVertexData {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let mut data = [0u8; 8];
    read.read_exact(&mut data)?;
    Ok(Self {
      _unknown: data
    })
  }
}

impl Model {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let name = read.read_fixed_length_null_terminated_string(64).unwrap();
    let model_type = read.read_i32()?;
    let bounding_radius = read.read_f32()?;
    let meshes_count = read.read_i32()?;
    let mesh_index = read.read_i32()?;

    let vertices_count = read.read_i32()?;
    let vertex_index = read.read_i32()?;
    let tangents_index = read.read_i32()?;

    let attachments_count = read.read_i32()?;
    let attachment_index = read.read_i32()?;

    let eye_balls_count = read.read_i32()?;
    let eye_ball_index = read.read_i32()?;

    let vertex_data = ModelVertexData::read(read)?;
    for _ in 0..8 {
      read.read_i32()?;
    }

    Ok(Self {
      name,
      model_type,
      bounding_radius,
      meshes_count,
      mesh_index: mesh_index as u64,
      vertices_count,
      vertex_index,
      tangents_index,
      attachment_index,
      attachments_count,
      eye_ball_index,
      eye_balls_count,
      vertex_data
    })
  }
}
