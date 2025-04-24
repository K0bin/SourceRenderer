#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
extern crate io_util;

mod anim_desc;
mod body_part;
mod bone;
mod bone_controller;
mod header;
mod header2;
mod hitbox_set;
mod mesh;
mod model;
mod model_file;
mod sequence_desc;
mod skin_replacement;
mod texture;

pub use self::anim_desc::AnimDesc;
pub use self::body_part::BodyPart;
pub use self::bone::Bone;
pub use self::bone_controller::BoneController;
pub use self::header::{Header, StudioHDRFlags};
pub use self::header2::Header2;
pub use self::hitbox_set::HitboxSet;
pub use self::io_util::*;
pub use self::mesh::{Mesh, MeshVertexData};
pub use self::model::{Model, ModelVertexData};
pub use self::model_file::ModelFile;
pub use self::sequence_desc::SequenceDesc;
pub use self::skin_replacement::{SkinReplacementTable, SkinReplacementTableEntry};
pub use self::texture::Texture;
