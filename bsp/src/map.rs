use Lump;

pub struct Map {
    //pub name: str,
    pub identifier: i32,
    pub version: i32,
    pub lumps: [Lump; 63],
}