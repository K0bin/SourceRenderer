use std::io::{Read, Result as IOResult};

use bevy_math::{Vec2, Vec3};

use crate::{BoneWeight, PrimitiveRead};

#[derive(Clone)]
pub struct Vertex {
    pub bone_weights: BoneWeight,
    pub vec_position: Vec3,
    pub vec_normal: Vec3,
    pub vec_tex_coord: Vec2,
}

impl Vertex {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let bone_weights = BoneWeight::read(read)?;
        let vec_position = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
        let vec_normal = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
        let vec_tex_coord = Vec2::new(read.read_f32()?, read.read_f32()?);
        Ok(Self {
            bone_weights,
            vec_position,
            vec_normal,
            vec_tex_coord,
        })
    }
}
