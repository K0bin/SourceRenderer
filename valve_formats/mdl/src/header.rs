use std::io::{Read, Result as IOResult};
use std::ffi::CString;
use std::os::raw::c_char;

use nalgebra::Vector3;

use crate::PrimitiveRead;

bitflags! {
  pub struct StudioHDRFlags: u32 {
    const AUTO_GENERATED_HITBOX = 1;
    const USES_ENV_CUBEMAP = 1 << 1;
    const FORCE_OPAQUE = 1 << 2;
    const TRANSLUCENT_TWOPASS = 1 << 3;
    const STATIC_PROP = 1 << 4;
    const USES_FB_TEXTURE = 1 << 5;
    const HAS_SHADOW_LOD = 1 << 6;
    const USES_BUMP_MAPPING = 1 << 7;
    const USE_SHADOW_LOD_MATERIALS = 1 << 8;
    const OBSOLETE = 1 << 9;
    const UNUNSED = 1 << 10;
    const NO_FORCED_FADE = 1 << 11;
    const FORCE_PHONEME_CROSSFADE = 1 << 12;
    const CONSTANT_DIRECTIONAL_LIGHT_DOT = 1 << 13;
    const FLEXES_CONVERTED = 1 << 14;
    const BUILT_IN_PREVIEW_MODE = 1 << 16;
    const AMBIENT_BOOST = 1 << 16;
    const DO_NOT_CAST_SHADOWS = 1 << 17;
    const CAST_TEXTURE_SHADOWS = 1 << 18;
  }
}

pub struct Header {
  pub id: i32,
  pub version: i32,
  pub checksum: i32,
  pub name: String,

  pub data_length: i32,

  pub eye_position: Vector3<f32>,
  pub illum_position: Vector3<f32>,
  pub hull_min: Vector3<f32>,
  pub hull_max: Vector3<f32>,
  pub view_bb_min: Vector3<f32>,
  pub view_bb_max: Vector3<f32>,

  pub flags: StudioHDRFlags,

  pub bone_count: i32,
  pub bone_offset: i32,

  pub bone_controller_count: i32,
  pub bone_controller_offset: i32,

  pub hitbox_count: i32,
  pub hitbox_offset: i32,

  pub local_anim_count: i32,
  pub local_anim_offset: i32,

  pub local_seq_count: i32,
  pub local_seq_offset: i32,

  pub activity_list_version: i32,
  pub events_indexed: i32,

  pub texture_count: i32,
  pub texture_offset: i32,

  pub texture_dir_count: i32,
  pub texture_dir_offset: i32,

  pub skin_reference_count: i32,
  pub skin_reference_family_count: i32,
  pub skin_reference_index: i32,

  pub body_part_count: i32,
  pub body_part_offset: i32,

  pub attachment_count: i32,
  pub attachment_offset: i32,

  pub local_node_count: i32,
  pub local_node_index: i32,
  pub local_node_name_index: i32,

  pub flex_desc_count: i32,
  pub flex_desc_index: i32,

  pub flex_controller_count: i32,
  pub flex_controller_index: i32,

  pub flex_rules_count: i32,
  pub flex_rules_index: i32,

  pub ik_chain_count: i32,
  pub ik_chain_index: i32,

  pub mouths_count: i32,
  pub mouths_index: i32,

  pub local_pose_param_count: i32,
  pub local_pose_param_index: i32,

  pub surface_prop_index: i32,

  pub key_value_index: i32,
  pub key_value_count: i32,

  pub ik_lock_count: i32,
  pub ik_lock_index: i32,

  pub mass: f32,
  pub contents: i32,

  pub include_model_count: i32,
  pub include_model_index: i32,

  pub virtual_model: i32,

  pub anim_blocks_name_index: i32,
  pub anim_blocks_count: i32,
  pub anim_blocks_index: i32,

  pub anim_block_model: i32,

  pub bone_table_name_index: i32,

  pub vertex_base: i32,
  pub offset_base: i32,

  pub directional_dot_product: u8,

  pub root_lod: u8,

  pub allowed_root_lods_count: u8,

  pub flex_controller_ui_count: i32,
  pub flex_controller_ui_index: i32,

  pub studio_hdr2_index: i32,
}

impl Header {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let id = read.read_i32()?;
    let version = read.read_i32()?;
    let checksum = read.read_i32()?;
    let mut name_data = [0u8; 64];
    read.read_exact(&mut name_data)?;
    let name = unsafe { CString::from_raw(name_data.as_mut_ptr() as *mut c_char) }
      .to_str().unwrap().to_string();

    let data_length = read.read_i32()?;

    let eye_position = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let illum_position = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let hull_min = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let hull_max = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let view_bb_min = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
    let view_bb_max = Vector3::<f32>::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);

    let flags_raw = read.read_u32()?;
    let flags = StudioHDRFlags::from_bits(flags_raw).unwrap();

    let bone_count = read.read_i32()?;
    let bone_offset = read.read_i32()?;

    let bone_controller_count = read.read_i32()?;
    let bone_controller_offset = read.read_i32()?;

    let hitbox_count = read.read_i32()?;
    let hitbox_offset = read.read_i32()?;

    let local_anim_count = read.read_i32()?;
    let local_anim_offset = read.read_i32()?;

    let local_seq_count = read.read_i32()?;
    let local_seq_offset = read.read_i32()?;

    let activity_list_version = read.read_i32()?;
    let events_indexed = read.read_i32()?;

    let texture_count = read.read_i32()?;
    let texture_offset = read.read_i32()?;

    let texture_dir_count = read.read_i32()?;
    let texture_dir_offset = read.read_i32()?;

    let skin_reference_count = read.read_i32()?;
    let skin_reference_family_count = read.read_i32()?;
    let skin_reference_index = read.read_i32()?;

    let body_part_count = read.read_i32()?;
    let body_part_offset = read.read_i32()?;

    let attachment_count = read.read_i32()?;
    let attachment_offset = read.read_i32()?;

    let local_node_count = read.read_i32()?;
    let local_node_index = read.read_i32()?;
    let local_node_name_index = read.read_i32()?;

    let flex_desc_count = read.read_i32()?;
    let flex_desc_index = read.read_i32()?;

    let flex_controller_count = read.read_i32()?;
    let flex_controller_index = read.read_i32()?;

    let flex_rules_count = read.read_i32()?;
    let flex_rules_index = read.read_i32()?;

    let ik_chain_count = read.read_i32()?;
    let ik_chain_index = read.read_i32()?;

    let mouths_count = read.read_i32()?;
    let mouths_index = read.read_i32()?;

    let local_pose_param_count = read.read_i32()?;
    let local_pose_param_index = read.read_i32()?;

    let surface_prop_index = read.read_i32()?;

    let key_value_index = read.read_i32()?;
    let key_value_count = read.read_i32()?;

    let ik_lock_count = read.read_i32()?;
    let ik_lock_index = read.read_i32()?;

    let mass = read.read_f32()?;
    let contents = read.read_i32()?;

    let include_model_count = read.read_i32()?;
    let include_model_index = read.read_i32()?;

    let virtual_model = read.read_i32()?;

    let anim_blocks_name_index = read.read_i32()?;
    let anim_blocks_count = read.read_i32()?;
    let anim_blocks_index = read.read_i32()?;

    let anim_block_model = read.read_i32()?;

    let bone_table_name_index = read.read_i32()?;

    let vertex_base = read.read_i32()?;
    let offset_base = read.read_i32()?;

    let directional_dot_product = read.read_u8()?;

    let root_lod = read.read_u8()?;

    let allowed_root_lods_count = read.read_u8()?;

    let _unused = read.read_u8()?;
    let _unused1 = read.read_i32()?;

    let flex_controller_ui_count = read.read_i32()?;
    let flex_controller_ui_index = read.read_i32()?;

    let studio_hdr2_index = read.read_i32()?;

    let _unused2 = read.read_i32()?;

    Ok(Self {
      id,
      version,
      checksum,
      name,
      data_length,
      eye_position,
      illum_position,
      hull_min,
      hull_max,
      view_bb_min,
      view_bb_max,
      flags,
      bone_count,
      bone_offset,
      bone_controller_count,
      bone_controller_offset,
      hitbox_count,
      hitbox_offset,
      local_anim_count,
      local_anim_offset,
      local_seq_count,
      local_seq_offset,
      activity_list_version,
      events_indexed,
      texture_count,
      texture_offset,
      texture_dir_count,
      texture_dir_offset,
      skin_reference_count,
      skin_reference_family_count,
      skin_reference_index,
      body_part_count,
      body_part_offset,
      attachment_count,
      attachment_offset,
      local_node_count,
      local_node_index,
      local_node_name_index,
      flex_desc_count,
      flex_desc_index,
      flex_controller_count,
      flex_controller_index,
      flex_rules_count,
      flex_rules_index,
      ik_chain_count,
      ik_chain_index,
      mouths_count,
      mouths_index,
      local_pose_param_count,
      local_pose_param_index,
      surface_prop_index,
      key_value_index,
      key_value_count,
      ik_lock_count,
      ik_lock_index,
      mass,
      contents,
      include_model_count,
      include_model_index,
      virtual_model,
      anim_blocks_name_index,
      anim_blocks_count,
      anim_blocks_index,
      anim_block_model,
      bone_table_name_index,
      vertex_base,
      offset_base,
      directional_dot_product,
      root_lod,
      allowed_root_lods_count,
      flex_controller_ui_count,
      flex_controller_ui_index,
      studio_hdr2_index
    })
  }
}