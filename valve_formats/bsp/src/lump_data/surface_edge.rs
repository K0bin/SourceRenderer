use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use std::io::{Read, Result as IOResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct SurfaceEdge {
    pub index: i32,
}

impl LumpData for SurfaceEdge {
    fn lump_type() -> LumpType {
        LumpType::SurfaceEdges
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        4
    }

    fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let edge = reader.read_i32()?;
        Ok(Self { index: edge })
    }
}
