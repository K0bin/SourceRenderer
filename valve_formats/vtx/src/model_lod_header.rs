use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct ModelLODHeader {
    pub meshes_count: i32,
    pub mesh_offset: i32,

    pub switch_point: f32,
}

impl ModelLODHeader {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let meshes_count = read.read_i32()?;
        let meshes_offset = read.read_i32()?;
        let switch_point = read.read_f32()?;
        Ok(Self {
            meshes_count,
            mesh_offset: meshes_offset,
            switch_point,
        })
    }
}
