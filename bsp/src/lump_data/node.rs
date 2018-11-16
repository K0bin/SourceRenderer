use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

pub const NODE_SIZE: u8 = 16;

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
        let plane_number = reader.read_i32::<LittleEndian>();
        if plane_number.is_err() {
            return Err(plane_number.err().unwrap());
        }

        let mut children: [i32; 2] = [0; 2];
        for i in 0..children.len() {
            let child = reader.read_i32::<LittleEndian>();
            if child.is_err() {
                return Err(child.err().unwrap());
            }
            children[i] = child.unwrap();
        }

        let mut mins: [i16; 3] = [0; 3];
        for i in 0..mins.len() {
            let min = reader.read_i16::<LittleEndian>();
            if min.is_err() {
                return Err(min.err().unwrap());
            }
            mins[i] = min.unwrap();
        }

        let mut maxs: [i16; 3] = [0; 3];
        for i in 0..maxs.len() {
            let max = reader.read_i16::<LittleEndian>();
            if max.is_err() {
                return Err(max.err().unwrap());
            }
            maxs[i] = max.unwrap();
        }

        let first_face = reader.read_u16::<LittleEndian>();
        if first_face.is_err() {
            return Err(first_face.err().unwrap());
        }

        let faces_count = reader.read_u16::<LittleEndian>();
        if faces_count.is_err() {
            return Err(faces_count.err().unwrap());
        }

        let area = reader.read_u16::<LittleEndian>();
        if area.is_err() {
            return Err(area.err().unwrap());
        }

        let padding = reader.read_u16::<LittleEndian>();
        if padding.is_err() {
            return Err(padding.err().unwrap());
        }

        return Ok(Node {
            plane_number: plane_number.unwrap(),
            children: children,
            mins: mins,
            maxs: maxs,
            first_face: first_face.unwrap(),
            faces_count: faces_count.unwrap(),
            area: area.unwrap(),
            padding: padding.unwrap()
        });
    }
}
