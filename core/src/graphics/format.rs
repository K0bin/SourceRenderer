
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
  Unknown,
  R32,
  R16,
  RGBA8,
  BGR8UNorm,
  BGRA8UNorm,
  DXT1,
  DXT1Alpha,
  DXT3,
  DXT5,
  R16Float,
  R32Float,
  RG32Float,
  RGB32Float,
  RGBA32Float,

  D16,
  D16S8,
  D32,
  D32S8,
  D24S8
}

impl Format {
  pub fn is_depth(&self) -> bool {
    matches!(self,
      Format::D32
      | Format::D16
      | Format::D16S8
      | Format::D24S8
      | Format::D32S8)
  }

  pub fn is_stencil(&self) -> bool {
    matches!(self,
      Format::D16S8
      | Format::D24S8
      | Format::D32S8)
  }
}