#![allow(dead_code)]

pub use self::lump::Lump;
pub use self::map_header::MapHeader;
pub use self::map::Map;
pub use self::lump_data::*;

extern crate byteorder;
extern crate num_traits;
#[macro_use]
extern crate num_derive;

#[macro_use]
extern crate bitflags;

extern crate nalgebra;

mod lump;
mod lump_data;
mod map_header;
mod map;
