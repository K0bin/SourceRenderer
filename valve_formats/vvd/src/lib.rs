extern crate bitflags;
extern crate io_util;

mod header;
mod fixup_table;
mod vertex;
mod bone_weight;
mod tangent;

pub use self::io_util::*;
pub use self::header::Header;
pub use self::fixup_table::VertexFileFixup;
pub use self::vertex::Vertex;
pub use self::bone_weight::BoneWeight;
pub use self::tangent::Tangent;
