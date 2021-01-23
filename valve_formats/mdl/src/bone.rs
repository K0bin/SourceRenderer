use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::{Vector3, Vector4, Quaternion, Matrix3x4};

use crate::PrimitiveRead;

pub struct Bone {
  pub name_index: i32,
  pub parent: i32,
  pub bone_controller: [i32; 6],

  pub position: Vector3<f32>,
  pub quaternion: Quaternion<f32>,
  pub rotation: Vector3<f32>,

  pub pos_scale: Vector3<f32>,
  pub rot_scale: Vector3<f32>,

  pub pose_to_bone: Matrix3x4<f32>,
  pub alignment: Quaternion<f32>,

  pub flags: i32,
  pub proc_type: i32,
  pub proc_index: i32,
  pub surface_prop_index: i32,
  pub contents: i32
}

impl Bone {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let name_index = read.read_i32()?;
    let parent = read.read_i32()?;
    let mut bone_controller = [0i32; 6];
    for i in 0..bone_controller.len() {
      bone_controller[i] = read.read_i32()?;
    }

    let position = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let axis = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let scale = read.read_f32()?;
    let quaternion = Quaternion::from_parts(scale, axis);
    let rotation = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let pos_scale = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let rot_scale = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let pose_to_bone = Matrix3x4::new(read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?,
                                      read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?,
                                      read.read_f32()?, read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let axis = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let scale = read.read_f32()?;
    let alignment = Quaternion::from_parts(scale, axis);

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
