use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

pub const BRUSH_SIZE: u8 = 12;

#[derive(Copy, Clone, Debug, Default)]
pub struct Brush {
    pub first_side: i32,
    pub sides_count: i32,
    pub contents: BrushContents
}

bitflags! {
    #[derive(Default)]
    pub struct BrushContents: u32 {
        const Empty = 0;
        const Solid = 0x1;
        const Window = 0x2;
        const Aux = 0x4;
        const Grate = 0x8;
        const Slime = 0x10;
        const Water = 0x20;
        const Mist = 0x40;
        const Opaque = 0x80;
        const TestFogVolume = 0x100;
        const Unused = 0x200;
        const Unused6 = 0x400;
        const Team1 = 0x800;
        const Team2 = 0x1000;
        const IgnoreNodrawOpaque = 0x2000;
        const Movable = 0x4000;
        const AreaPortal = 0x8000;
        const Playerclip = 0x10000;
        const Monsterclip = 0x20000;
        const Current0 = 0x40000;
        const Current90 = 0x80000;
        const Current180 = 0x100000;
        const Current270 = 0x200000;
        const CurrentUp = 0x400000;
        const CurrentDown = 0x800000;
        const Origin = 0x1000000;
        const Monster = 0x2000000;
        const Debris = 0x4000000;
        const Detail = 0x8000000;
        const Translucent = 0x10000000;
        const Ladder = 0x20000000;
        const Hitbox = 0x40000000;
    }
}

impl BrushContents {
    pub fn new(bits: u32) -> BrushContents {
        return BrushContents {
            bits: bits
        };
    }
}

impl Brush {
    pub fn read(reader: &mut Read) -> Result<Brush, Error> {
        let first_side = reader.read_i32::<LittleEndian>();
        if first_side.is_err() {
            return Err(first_side.err().unwrap());
        }
        let sides_count = reader.read_i32::<LittleEndian>();
        if sides_count.is_err() {
            return Err(sides_count.err().unwrap());
        }
        let contents = reader.read_u32::<LittleEndian>();
        if contents.is_err() {
            return Err(contents.err().unwrap());
        }
        return Ok(Brush {
            first_side: first_side.unwrap(),
            sides_count: sides_count.unwrap(),
            contents: BrushContents { bits: contents.unwrap() }
        });
    }
}
