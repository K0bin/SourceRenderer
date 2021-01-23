use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct Mesh {
  pub material: i32,
  pub model_index: i32,
  pub vertices_count: i32,
  pub vertex_offset: i32,
  pub flexes_count: i32,
  pub flex_index: i32,
  pub material_type: i32,
  pub material_param: i32,
  pub mesh_id: i32,
  pub center: Vector3<f32>,
  pub vertex_data: MeshVertexData
}

pub struct MeshVertexData {
  model_vertex_data: i32,
  lod_vertices: [i32; 8]
}

impl MeshVertexData {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let model_vertex_data = read.read_i32()?;
    let mut lod_vertices = [0i32; 8];
    for i in 0..8 {
      lod_vertices[i] = read.read_i32()?;
    }
    Ok(Self {
      model_vertex_data,
      lod_vertices
    })
  }
}

impl Mesh {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let material = read.read_i32()?;
    let model_index = read.read_i32()?;
    let vertices_count = read.read_i32()?;
    let vertex_offset = read.read_i32()?;
    let flexes_count = read.read_i32()?;
    let flex_index = read.read_i32()?;
    let material_type = read.read_i32()?;
    let material_param = read.read_i32()?;
    let mesh_id = read.read_i32()?;
    let center = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let vertex_data = MeshVertexData::read(read)?;
    for _ in 0..8 {
      read.read_i32()?;
    }
    Ok(Self {
      material,
      model_index,
      vertices_count,
      vertex_offset,
      flexes_count,
      flex_index,
      material_type,
      material_param,
      mesh_id,
      center,
      vertex_data
    })
  }
}
