use std::io::{Read, Result as IOResult};

use bevy_math::{Mat4, Quat, Vec3};

use crate::PrimitiveRead;

pub struct Bone {
  pub name_index: i32,
  pub parent: i32,
  pub bone_controller: [i32; 6],

  pub position: Vec3,
  pub quaternion: Quat,
  pub rotation: Vec3,

  pub pos_scale: Vec3,
  pub rot_scale: Vec3,

  pub pose_to_bone: Mat4,
  pub alignment: Quat,

  pub flags: i32,
  pub proc_type: i32,
  pub proc_index: i32,
  pub surface_prop_index: i32,
  pub contents: i32
}

impl Bone {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let name_index = read.read_i32()?;
    let parent = read.read_i32()?;
    let mut bone_controller = [0i32; 6];
    for i in 0..bone_controller.len() {
      bone_controller[i] = read.read_i32()?;
    }

    let position = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let axis = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let scale = read.read_f32()?;
    let quaternion = Quat::from_scaled_axis(axis * scale);
    let rotation = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let pos_scale = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let rot_scale = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let pose_to_bone = Mat4::from_cols_slice(&[read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?,
                                      read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?,
                                      read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?]);
    let axis = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let scale = read.read_f32()?;
    let alignment = Quat::from_scaled_axis(axis * scale);

    let flags = read.read_i32()?;
    let proc_type = read.read_i32()?;
    let proc_index = read.read_i32()?;
    let surface_prop_index = read.read_i32()?;
    let contents = read.read_i32()?;

    for _ in 0..8 {
      let _ = read.read_u32()?;
    }

    Ok(Self {
      name_index,
      parent,
      bone_controller,
      position,
      quaternion,
      rotation,
      pos_scale,
      rot_scale,
      pose_to_bone,
      alignment,
      flags,
      proc_type,
      proc_index,
      surface_prop_index,
      contents
    })
  }
}
