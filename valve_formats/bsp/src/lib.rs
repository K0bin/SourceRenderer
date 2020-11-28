#![allow(dead_code)]

pub use self::lump::Lump;
pub use self::map_header::MapHeader;
pub use self::map::Map;
pub use self::lump_data::*;

#[macro_use]
extern crate bitflags;

extern crate nalgebra;

mod lump;
mod lump_data;
mod map_header;
mod map;
mod read_util;

pub(crate) use self::read_util::*;
