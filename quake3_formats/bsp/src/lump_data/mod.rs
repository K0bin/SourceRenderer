use std::io::{Read, Result as IOResult};

pub use crate::lump_data::brush_model::BrushModel;
pub use crate::lump_data::face::Face;
pub use crate::lump_data::vertex::Vertex;

mod brush_model;
mod face;
mod vertex;

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum LumpType {
    Entities = 0,
    Textures = 1,
    Planes = 2,
    Nodes = 3,
    Leafs = 4,
    LeafFaces = 5,
    LeafBrushes = 6,
    Models = 7,
    Brushes = 8,
    BrushSides = 9,
    Vertices = 10,
    MeshVerts = 11,
    Effects = 12,
    Faces = 13,
    Lightmaps = 14,
    LightVols = 15,
    VisData = 16,
}

pub(crate) trait LumpData: Sized {
    fn lump_type() -> LumpType;
    fn element_size(version: i32) -> usize;
    fn read(read: &mut dyn Read, version: i32) -> IOResult<Self>;
}
