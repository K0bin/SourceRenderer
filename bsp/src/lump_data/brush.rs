use bsp_read::BspRead;
use std::io::Read;

pub struct Brush {
    pub first_side: i32,
    pub sides_count: i32,
    pub contents: BrushContents
}

pub enum BrushContents {
    Empty = 0,
    Solid = 0x1,
    Window = 0x2,
    Aux = 0x4,
    Grate = 0x8,
    Slime = 0x10,
    Water = 0x20,
    Mist = 0x40,
    Opaque = 0x80,
    TestFogVolume = 0x100,
    Unused = 0x200,
    Unused6 = 0x400,
    Team1 = 0x800,
    Team2 = 0x1000,
    IgnoreNodrawOpaque = 0x2000,
    Movable = 0x4000,
    AreaPortal = 0x8000,
    Playerclip = 0x10000,
    Monsterclip = 0x20000,
    Current0 = 0x40000,
    Current90 = 0x80000,
    Current180 = 0x100000,
    Current270 = 0x200000,
    CurrentUp = 0x400000,
    CurrentDown = 0x800000,
    Origin = 0x1000000,
    Monster = 0x2000000,
    Debris = 0x4000000,
    Detail = 0x8000000,
    Translucent = 0x10000000,
    Ladder = 0x20000000,
    Hitbox = 0x40000000
}
