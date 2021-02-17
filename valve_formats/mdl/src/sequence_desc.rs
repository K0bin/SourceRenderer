use std::io::{Read, Result as IOResult};

use nalgebra::Vector3;

use crate::PrimitiveRead;

pub struct SequenceDesc {
  pub base_ptr: i32,

  pub label_index: i32,

  pub activity_name_index: i32,

  pub flags: i32,

  pub activity: i32,
  pub activity_weight: i32,

  pub events_count: i32,
  pub event_index: i32,

  pub bb_min: Vector3<f32>,
  pub bb_max: Vector3<f32>,

  pub blends_count: i32,
  pub anim_index_index: i32,

  pub movement_index: i32,
  pub group_size: [i32; 2],
  pub param_index: [i32; 2],
  pub param_start: [f32; 2],
  pub param_end: [f32; 2],
  pub param_parent: i32,

  pub fade_in_time: f32,
  pub fade_out_time: f32,

  pub local_entry_node: i32,
  pub local_exit_node: i32,
  pub node_flags: i32,

  pub entry_phase: f32,
  pub exit_phase: f32,

  pub last_frame: f32,

  pub next_sequence: i32,
  pub pose: i32,

  pub ik_rules_count: i32,

  pub auto_layers_count: i32,
  pub auto_layer_index: i32,

  pub weight_list_index: i32,

  pub pose_key_index: i32,

  pub ik_locks_count: i32,
  pub ik_lock_index: i32,

  pub key_value_index: i32,
  pub key_value_size: i32,

  pub cycle_pose_index: i32
}

impl SequenceDesc {
  pub fn read(read: &mut dyn Read) -> IOResult<Self> {
    let base_ptr = read.read_i32()?;

    let label_index = read.read_i32()?;

    let activity_name_index = read.read_i32()?;

    let flags = read.read_i32()?;

    let activity = read.read_i32()?;
    let activity_weight = read.read_i32()?;

    let events_count = read.read_i32()?;
    let event_index = read.read_i32()?;

    let bb_min = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let bb_max = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let blends_count = read.read_i32()?;
    let anim_index_index = read.read_i32()?;

    let movement_index = read.read_i32()?;
    let group_size = [read.read_i32()?, read.read_i32()?];
    let param_index = [read.read_i32()?, read.read_i32()?];
    let param_start = [read.read_f32()?, read.read_f32()?];
    let param_end = [read.read_f32()?, read.read_f32()?];
    let param_parent = read.read_i32()?;

    let fade_in_time = read.read_f32()?;
    let fade_out_time = read.read_f32()?;

    let local_entry_node = read.read_i32()?;
    let local_exit_node = read.read_i32()?;
    let node_flags = read.read_i32()?;

    let entry_phase = read.read_f32()?;
    let exit_phase = read.read_f32()?;

    let last_frame = read.read_f32()?;

    let next_sequence = read.read_i32()?;
    let pose = read.read_i32()?;

    let ik_rules_count = read.read_i32()?;

    let auto_layers_count = read.read_i32()?;
    let auto_layer_index = read.read_i32()?;

    let weight_list_index = read.read_i32()?;

    let pose_key_index = read.read_i32()?;

    let ik_locks_count = read.read_i32()?;
    let ik_lock_index = read.read_i32()?;

    let key_value_index = read.read_i32()?;
    let key_value_size = read.read_i32()?;

    let cycle_pose_index = read.read_i32()?;

    for _ in 0..7 {
      read.read_i32()?;
    }

      Ok(Self {
        base_ptr,
        label_index,
        activity_name_index,
        flags,
        activity,
        activity_weight,
        events_count,
        event_index,
        bb_min,
        bb_max,
        blends_count,
        anim_index_index,
        movement_index,
        group_size,
        param_index,
        param_start,
        param_end,
        param_parent,
        fade_in_time,
        fade_out_time,
        local_entry_node,
        local_exit_node,
        node_flags,
        entry_phase,
        exit_phase,
        last_frame,
        next_sequence,
        pose,
        ik_rules_count,
        auto_layers_count,
        auto_layer_index,
        weight_list_index,
        pose_key_index,
        ik_locks_count,
        ik_lock_index,
        key_value_index,
        key_value_size,
        cycle_pose_index
    })
  }
}
