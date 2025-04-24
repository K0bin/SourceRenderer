use crate::image_format::ImageFormat;

pub struct Thumbnail {
    pub data: Box<[u8]>,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}
