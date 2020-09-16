use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

pub const NODE_SIZE: u8 = 32;

#[derive(Copy, Clone, Debug, Default)]
pub struct Node {
    pub plane_number: i32,
    pub children: [i32; 2],
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_face: u16,
    pub faces_count: u16,
    pub area: u16,
    pub padding: u16
}

impl Node {
    pub fn read(reader: &mut Read) -> Result<Node, Error> {
        let plane_number = reader.read_i32::<LittleEndian>()?;
        let mut children: [i32; 2] = [0; 2];
        for i in 0..children.len() {
            let child = reader.read_i32::<LittleEndian>()?;
            children[i] = child;
        }

        let mut mins: [i16; 3] = [0; 3];
        for i in 0..mins.len() {
            mins[i] = reader.read_i16::<LittleEndian>()?;
        }

        let mut maxs: [i16; 3] = [0; 3];
        for i in 0..maxs.len() {
            maxs[i] = reader.read_i16::<LittleEndian>()?;
        }

        let first_face = reader.read_u16::<LittleEndian>()?;
        let faces_count = reader.read_u16::<LittleEndian>()?;
        let area = reader.read_u16::<LittleEndian>()?;
        let padding = reader.read_u16::<LittleEndian>()?;

        return Ok(Node {
            plane_number,
            children,
            mins,
            maxs,
            first_face,
            faces_count,
            area,
            padding
        });
    }
}
