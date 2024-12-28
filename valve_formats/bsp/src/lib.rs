#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
pub extern crate zip;
extern crate io_util;

pub use self::lump::Lump;
pub use self::map_header::MapHeader;
pub use self::map::Map;
pub use self::lump_data::*;

mod lump;
mod lump_data;
mod map_header;
mod map;

pub(crate) use io_util::*;
