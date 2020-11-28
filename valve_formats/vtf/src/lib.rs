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

use self::read_util::*;
use self::image_format::FormatSizeInfo;
use self::image_format::ImageFormatInfo;
use self::image_format::calculate_image_size;
use self::image_format::is_image_format_supported;

pub use self::image_format::ImageFormat;
pub use self::texture_data::*;
pub use self::header::Header;
pub use self::texture_flags::TextureFlags;
pub use self::texture::VtfTexture;
