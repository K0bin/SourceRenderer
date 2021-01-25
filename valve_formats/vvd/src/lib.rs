#[macro_use]
extern crate bitflags;

mod header;
mod fixup_table;
mod vertex;
mod bone_weight;
mod tangent;
mod read_util;

pub use self::read_util::*;
pub use self::header::Header;
pub use self::fixup_table::VertexFileFixup;
pub use self::vertex::Vertex;
pub use self::bone_weight::BoneWeight;
pub use self::tangent::Tangent;
