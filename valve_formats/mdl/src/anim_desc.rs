use std::io::{Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct AnimDesc {
  pub base_ptr: i32,
  pub name_index: i32,

  pub fps: f32,
  pub flags: i32,

  pub frames_count: i32,
  pub movements_count: i32,
  pub movement_index: i32,

  pub anim_block: i32,
  pub anim_index: i32,

  pub ik_rules_count: i32,
  pub ik_rule_index: i32,
  pub anim_block_ik_rule_index: i32,

  pub local_hierarchy_indices_count: i32,
  pub local_hierarchy_index: i32,

  pub section_index: i32,
  pub section_frames: i32,

  pub zero_frame_span: i16,
  pub zero_frame_count: i16,
  pub zero_frame_index: i32,

  pub zero_frame_stall_time: f32
}

impl AnimDesc {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let base_ptr = read.read_i32()?;
    let name_index = read.read_i32()?;

    let fps = read.read_f32()?;
    let flags = read.read_i32()?;

    let frames_count = read.read_i32()?;
    let movements_count = read.read_i32()?;
    let movement_index = read.read_i32()?;

    //unknown
    for _ in 0..6 {
      read.read_i32()?;
    }

    let anim_block = read.read_i32()?;
    let anim_index = read.read_i32()?;

    let ik_rules_count = read.read_i32()?;
    let ik_rule_index = read.read_i32()?;
    let anim_block_ik_rule_index = read.read_i32()?;

    let local_hierarchy_indices_count = read.read_i32()?;
    let local_hierarchy_index = read.read_i32()?;

    let section_index = read.read_i32()?;
    let section_frames = read.read_i32()?;

    let zero_frame_span  = read.read_i16()?;
    let zero_frame_count = read.read_i16()?;
    let zero_frame_index = read.read_i32()?;

    let zero_frame_stall_time = read.read_f32()?;

    Ok(Self {
      base_ptr,
      name_index,
      fps,
      flags,
      frames_count,
      movements_count,
      movement_index,
      anim_block,
      anim_index,
      ik_rules_count,
      ik_rule_index,
      anim_block_ik_rule_index,
      local_hierarchy_indices_count,
      local_hierarchy_index,
      section_index,
      section_frames,
      zero_frame_span,
      zero_frame_count,
      zero_frame_index,
      zero_frame_stall_time
    })
  }
}
