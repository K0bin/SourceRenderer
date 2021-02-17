use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct VertexFileFixup {
  pub lod: i32,
  pub source_vertex_id: i32,
  pub vertexes_count: i32
}

impl VertexFileFixup {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let lod = read.read_i32()?;
    let source_vertex_id = read.read_i32()?;
    let vertexes_count = read.read_i32()?;
    Ok(Self {
      lod,
      source_vertex_id,
      vertexes_count
    })
  }
}
