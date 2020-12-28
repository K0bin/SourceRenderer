use crate::{LumpType, LumpData};
use crate::PrimitiveRead;
use std::io::{Read, Result as IOResult};

pub struct DispTri {
  pub tags: DispTriTags
}

impl LumpData for DispTri {
  fn lump_type() -> LumpType {
    LumpType::DisplacementTriangles
  }

  fn element_size(_version: i32) -> usize {
    2
  }

  fn read(mut read: &mut dyn Read, version: i32) -> IOResult<Self> {
    let tags = read.read_u16()?;
    Ok(Self {
      tags: DispTriTags::from_bits(tags).unwrap()
    })
  }
}

bitflags! {
  #[derive(Default)]
  pub struct DispTriTags : u16 {
    const EMPTY = 0;
    const SURFACE = 1;
    const WALKABLE = 2;
    const BUILDABLE = 4;
    const FLAG_SURFPROP1 = 8;
    const FLAG_SURFPROP2 = 16;
  }
}

