use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;

#[derive(Copy, Clone, Debug, Default)]
pub struct Brush {
  pub first_side: i32,
  pub sides_count: i32,
  pub contents: BrushContents
}

bitflags! {
  #[derive(Default)]
  pub struct BrushContents: u32 {
    const EMPTY = 0;
    const SOLID = 0x1;
    const WINDOW = 0x2;
    const AUX = 0x4;
    const GRATE = 0x8;
    const SLIME = 0x10;
    const WATER = 0x20;
    const MIST = 0x40;
    const OPAQUE = 0x80;
    const TEST_FOG_VOLUME = 0x100;
    const UNUSED = 0x200;
    const UNUSED6 = 0x400;
    const TEAM1 = 0x800;
    const TEAM2 = 0x1000;
    const IGNORE_NO_DRAW_OPAQUE = 0x2000;
    const MOVABLE = 0x4000;
    const AREA_PORTAL = 0x8000;
    const PLAYER_CLIP = 0x10000;
    const MONSTER_CLIP = 0x20000;
    const CURRENT0 = 0x40000;
    const CURRENT90 = 0x80000;
    const CURRENT180 = 0x100000;
    const CURRENT270 = 0x200000;
    const CURRENT_UP = 0x400000;
    const CURRENT_DOWN = 0x800000;
    const ORIGIN = 0x1000000;
    const MONSTER = 0x2000000;
    const DEBRIS = 0x4000000;
    const DETAIL = 0x8000000;
    const TRANSLUCENT = 0x10000000;
    const LADDER = 0x20000000;
    const HITBOX = 0x40000000;
  }
}

impl BrushContents {
  pub fn new(bits: u32) -> BrushContents {
    return BrushContents {
      bits
    };
  }
}

impl LumpData for Brush {
  fn lump_type() -> LumpType {
    LumpType::Brushes
  }
  fn lump_type_hdr() -> Option<LumpType> {
    None
  }

  fn element_size(_version: i32) -> usize {
    12
  }

  fn read(reader: &mut dyn Read, _version: i32) -> IOResult<Self> {
    let first_side = reader.read_i32()?;
    let sides_count = reader.read_i32()?;
    let contents = reader.read_u32()?;
    return Ok(Self {
      first_side,
      sides_count,
        contents: BrushContents { bits: contents }
    });
  }
}
