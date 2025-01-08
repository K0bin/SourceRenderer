#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
extern crate io_util;

mod header;
mod header2;
mod texture;
mod skin_replacement;
mod bone;
mod bone_controller;
mod hitbox_set;
mod anim_desc;
mod sequence_desc;
mod model_file;
mod body_part;
mod model;
mod mesh;

pub use self::io_util::*;
pub use self::header::{Header, StudioHDRFlags};
pub use self::header2::Header2;
pub use self::texture::Texture;
pub use self::skin_replacement::{SkinReplacementTableEntry, SkinReplacementTable};
pub use self::bone::Bone;
pub use self::bone_controller::BoneController;
pub use self::hitbox_set::HitboxSet;
pub use self::anim_desc::AnimDesc;
pub use self::sequence_desc::SequenceDesc;
pub use self::model_file::ModelFile;
pub use self::body_part::BodyPart;
pub use self::model::{Model, ModelVertexData};
pub use self::mesh::{Mesh, MeshVertexData};
