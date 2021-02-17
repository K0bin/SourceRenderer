use crate::image_format::ImageFormat;

pub struct MipMap {
  pub frames: Vec<Frame>,
  pub format: ImageFormat,
  pub width: u32,
  pub height: u32
}

pub struct Frame {
  pub faces: Vec<Face>
}

pub struct Face {
  pub slices: Vec<Slice>
}

pub struct Slice {
  pub data: Box<[u8]>
}
