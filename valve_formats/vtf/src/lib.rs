#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
extern crate io_util;
#[macro_use]
extern crate lazy_static;

mod header;
mod image_format;
mod texture;
mod texture_data;
mod texture_flags;
mod thumbnail;

pub use self::header::Header;
pub use self::image_format::ImageFormat;
pub use self::texture::VtfTexture;
pub use self::texture_data::*;
pub use self::texture_flags::TextureFlags;
