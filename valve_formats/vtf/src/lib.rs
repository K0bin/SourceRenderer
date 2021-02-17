#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;

mod header;
mod texture_data;
mod image_format;
mod texture_flags;
mod thumbnail;
mod texture;
mod read_util;

pub use self::image_format::ImageFormat;
pub use self::texture_data::*;
pub use self::header::Header;
pub use self::texture_flags::TextureFlags;
pub use self::texture::VtfTexture;
