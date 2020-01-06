
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
  Unknown,
  R32,
  R16,
  RGBA8,
  BGR8UNorm,
  BGRA8UNorm,
  DXT3,
  DXT5,
  R32Float,
  RG32Float,
  RGB32Float,
}
