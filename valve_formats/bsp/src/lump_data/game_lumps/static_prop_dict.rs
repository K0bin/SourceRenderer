use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::{PrimitiveRead, StringRead};
use std::ffi::CString;
use nalgebra::Vector3;
use crate::lump_data::leaf::ColorRGBExp32;

pub struct StaticPropDict {
  names: Box<[String]>,
  leaves: Box<[u16]>,
  props: Box<[StaticProp]>
}

impl StaticPropDict {
  pub fn id() -> u32 {
    //let id = unsafe { *("sprp".as_ptr() as *const u32) };
    //println!("sprp: {:?}", id);
    1936749168
  }

  pub fn read(mut read: &mut dyn Read, version: u16) -> IOResult<Self> {
    let dict_entries = read.read_i32()?;
    let mut names = Vec::<String>::with_capacity(dict_entries as usize);
    for _ in 0..dict_entries {
      let name = read.read_fixed_length_null_terminated_string(128).unwrap();
      names.push(name);
    }

    let leaf_count = read.read_i32()?;
    let mut leaves = Vec::<u16>::with_capacity(leaf_count as usize);
    for _ in 0..leaf_count {
      leaves.push(read.read_u16()?);
    }

    let prop_count = read.read_i32()?;
    let mut props = Vec::<StaticProp>::with_capacity(prop_count as usize);
    for _ in 0..prop_count {
      props.push(StaticProp::read(read, version)?);
    }

    Ok(Self {
      names: names.into_boxed_slice(),
      leaves: leaves.into_boxed_slice(),
      props: props.into_boxed_slice()
    })
  }
}

pub struct StaticProp {
  pub origin: Vector3<f32>,
  pub angles: Vector3<f32>,

  pub prop_type: u16,
  pub first_leaf: u16,
  pub leaf_count: u16,
  pub solid: u8,
  pub flags: u8,
  pub skin: i32,
  pub fade_min_dist: f32,
  pub fade_max_dist: f32,
  pub lighting_origin: Vector3<f32>,
  pub forced_fade_scale: f32,
  pub min_dx_level: u16,
  pub max_dx_level: u16,
  pub min_cpu_level: u8,
  pub max_cpu_level: u8,
  pub min_gpu_level: u8,
  pub max_gpu_level: u8,
  pub diffuse_modulation: ColorRGBExp32,
  pub disable_x360: bool,
  pub flags_ex: u32,
  pub uniform_scale: f32
}

impl StaticProp {
  pub fn read(mut read: &mut dyn Read, version: u16) -> IOResult<Self> {
    let origin = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let angles = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let prop_type = read.read_u16()?;
    let first_leaf = read.read_u16()?;
    let leaf_count = read.read_u16()?;
    let solid = read.read_u8()?;
    let flags = read.read_u8()?;
    let skin = read.read_i32()?;
    let fade_min_dist = read.read_f32()?;
    let fade_max_dist = read.read_f32()?;
    let lighting_origin = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let mut forced_fade_scale = 0f32;
    let mut min_dx_level = 0u16;
    let mut max_dx_level = 0u16;
    let mut min_cpu_level = 0u8;
    let mut max_cpu_level = 0u8;
    let mut min_gpu_level = 0u8;
    let mut max_gpu_level = 0u8;
    let mut diffuse_modulation = ColorRGBExp32::default();
    let mut disable_x360 = false;
    let mut flags_ex = 0u32;
    let mut uniform_scale = 1f32;
    if version >= 5 {
      forced_fade_scale = read.read_f32()?;
      if version == 6 || version == 7 {
        min_dx_level = read.read_u16()?;
        max_dx_level = read.read_u16()?;
      }
      if version >= 8 {
        min_cpu_level = read.read_u8()?;
        max_cpu_level = read.read_u8()?;
        min_gpu_level = read.read_u8()?;
        max_gpu_level = read.read_u8()?;
      }
      if version >= 7 {
        diffuse_modulation = ColorRGBExp32::read(read)?;
      }
      if version == 9 || version == 10 {
        disable_x360 = read.read_u32()? != 0;
      }
      if version >= 10 {
        flags_ex = read.read_u32()?;
        if version >= 11 {
          uniform_scale = read.read_f32()?;
        }
      }
    }

    Ok(Self {
      origin,
      angles,
      prop_type,
      first_leaf,
      leaf_count,
      solid,
      flags,
      skin,
      fade_min_dist,
      fade_max_dist,
      lighting_origin,
      forced_fade_scale,
      min_dx_level,
      max_dx_level,
      min_cpu_level,
      max_cpu_level,
      min_gpu_level,
      max_gpu_level,
      diffuse_modulation,
      disable_x360,
      flags_ex,
      uniform_scale
    })
  }
}
