use bevy_math::Vec3;
use std::io::{Error as IOError, ErrorKind, Read, Result as IOResult};

use crate::{LumpData, LumpType, PrimitiveRead};

pub struct DispInfo {
    pub start_position: Vec3,
    pub disp_vert_start: i32,
    pub disp_tri_start: i32,
    pub power: i32,
    pub min_tess: i32,
    pub smoothing_angle: f32,
    pub contents: i32,
    pub map_face: u16,
    pub lightmap_alpha_start: i32,
    pub lightmap_sample_position_start: i32,
    pub edge_neighbors: [DispNeighbor; 4],
    pub corner_neighbors: [DispCornerNeighbors; 4],
    pub allowed_verts: [u32; 10],
}

impl DispInfo {
    pub fn edge_neighbor(&self, edge: NeighborEdge) -> &DispNeighbor {
        let index: u8 = unsafe { std::mem::transmute(edge) };
        &self.edge_neighbors[index as usize]
    }
    pub fn corner_neighbor(&self, corner: NeighborCorner) -> &DispCornerNeighbors {
        let index: u8 = unsafe { std::mem::transmute(corner) };
        &self.corner_neighbors[index as usize]
    }
}

impl LumpData for DispInfo {
    fn lump_type() -> LumpType {
        LumpType::DisplacementInfo
    }
    fn lump_type_hdr() -> Option<LumpType> {
        None
    }

    fn element_size(_version: i32) -> usize {
        176
    }

    fn read(read: &mut dyn Read, _version: i32) -> IOResult<Self> {
        let start_position = Vec3::new(read.read_f32()?, read.read_f32()?, read.read_f32()?);
        let disp_vert_start = read.read_i32()?;
        let disp_tri_start = read.read_i32()?;
        let power = read.read_i32()?;
        if power != 2 && power != 3 && power != 4 {
            panic!("illegal power: {}", power);
        }
        let min_tess = read.read_i32()?;
        let smoothing_angle = read.read_f32()?;
        let contents = read.read_i32()?;
        let map_face = read.read_u16()?;
        let _padding = read.read_u16()?;
        let lightmap_alpha_start = read.read_i32()?;
        let lightmap_sample_position_start = read.read_i32()?;
        let edge_neighbors = [
            DispNeighbor::read(read)?,
            DispNeighbor::read(read)?,
            DispNeighbor::read(read)?,
            DispNeighbor::read(read)?,
        ];
        let corner_neighbors = [
            DispCornerNeighbors::read(read)?,
            DispCornerNeighbors::read(read)?,
            DispCornerNeighbors::read(read)?,
            DispCornerNeighbors::read(read)?,
        ];
        let mut allowed_verts = [0u32; 10];
        for i in 0..allowed_verts.len() {
            allowed_verts[i] = read.read_u32()?;
        }
        Ok(Self {
            start_position,
            disp_vert_start,
            disp_tri_start,
            power,
            min_tess,
            smoothing_angle,
            contents,
            map_face,
            lightmap_alpha_start,
            lightmap_sample_position_start,
            edge_neighbors,
            corner_neighbors,
            allowed_verts,
        })
    }
}

pub struct DispNeighbor {
    pub sub_neighbors: [DispSubNeighbor; 2],
}

impl DispNeighbor {
    pub fn any(&self) -> bool {
        self.sub_neighbors[0].is_valid() || self.sub_neighbors[1].is_valid()
    }

    pub fn corner_to_corner(&self) -> bool {
        self.sub_neighbors[0].is_valid()
            && self.sub_neighbors[0].span == NeighborSpan::CornerToCorner
    }

    pub fn simple_corner_to_corner(&self) -> bool {
        self.corner_to_corner()
            && self.sub_neighbors[0].neighbor_span == NeighborSpan::CornerToCorner
    }

    pub fn read(reader: &mut dyn Read) -> IOResult<Self> {
        let sub_neighbors = [
            DispSubNeighbor::read(reader)?,
            DispSubNeighbor::read(reader)?,
        ];
        Ok(Self { sub_neighbors })
    }
}

pub struct DispSubNeighbor {
    pub neighbor_index: u16,
    pub neighbor_orientation: NeighborOrientation,
    pub span: NeighborSpan,
    pub neighbor_span: NeighborSpan,
}

impl DispSubNeighbor {
    pub fn is_valid(&self) -> bool {
        self.neighbor_index != 0xffff
    }

    pub fn read(reader: &mut dyn Read) -> IOResult<Self> {
        let neighbor_index = reader.read_u16()?;
        let neighbor_orientation = reader.read_u8()?;

        if neighbor_orientation
            > unsafe {
                std::mem::transmute::<NeighborOrientation, u8>(
                    NeighborOrientation::CounterClockWise270,
                )
            }
            && neighbor_orientation
                != unsafe {
                    std::mem::transmute::<NeighborOrientation, u8>(NeighborOrientation::Unknown)
                }
        {
            return Err(IOError::new(
                ErrorKind::Other,
                format!(
                    "Value for neighbor orientation in DispSubNeighbor out of range: {}",
                    neighbor_orientation
                ),
            ));
        }

        let span = reader.read_u8()?;
        if span > unsafe { std::mem::transmute::<NeighborSpan, u8>(NeighborSpan::MidpointToCorner) }
        {
            println!(
                "{}",
                format!("Value for span in DispSubNeighbor out of range: {}", span)
            );
            // FIXME
        }

        let neighbor_span = reader.read_u8()?;
        if neighbor_span
            > unsafe { std::mem::transmute::<NeighborSpan, u8>(NeighborSpan::MidpointToCorner) }
        {
            println!(
                "{}",
                format!(
                    "Value for neighbor_span in DispSubNeighbor out of range: {}",
                    neighbor_span
                )
            );
            // FIXME
        }

        let _padding = reader.read_u8()?;
        Ok(Self {
            neighbor_index,
            neighbor_orientation: unsafe { std::mem::transmute(neighbor_orientation) },
            span: unsafe { std::mem::transmute(span) },
            neighbor_span: unsafe { std::mem::transmute(neighbor_span) },
        })
    }
}

pub struct DispCornerNeighbors {
    neighbors: [u16; 4],
    num_neighbors: u8,
}

impl DispCornerNeighbors {
    pub fn read(read: &mut dyn Read) -> IOResult<Self> {
        let neighbors = [
            read.read_u16()?,
            read.read_u16()?,
            read.read_u16()?,
            read.read_u16()?,
        ];
        let num_neighbors = read.read_u8()?;
        if num_neighbors > 4 {
            return Err(IOError::new(
                ErrorKind::Other,
                format!(
                    "Value for num_neighbors in DispCornerNeighbors out of range: {}",
                    num_neighbors
                ),
            ));
        }
        let _padding = read.read_u8()?;
        Ok(Self {
            neighbors,
            num_neighbors,
        })
    }

    pub fn corner_neighbor_indices(&self) -> &[u16] {
        &self.neighbors[..self.num_neighbors as usize]
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub enum NeighborOrientation {
    CounterClockwise0 = 0,
    CounterClockwise90 = 1,
    CounterClockwise180 = 2,
    CounterClockWise270 = 3,
    Unknown = 255,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub enum NeighborSpan {
    CornerToCorner = 0,
    CornerToMidpoint = 1,
    MidpointToCorner = 2,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub enum NeighborCorner {
    LowerLeft = 0,
    UpperLeft = 1,
    UpperRight = 2,
    LowerRight = 3,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub enum NeighborEdge {
    Left = 0,
    Top = 1,
    Right = 2,
    Bottom = 3,
}
