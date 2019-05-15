use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

#[derive(Copy, Clone, Debug, Default)]
pub struct Lump {
    pub file_offset: i32,
    pub file_length: i32,
    pub version: i32,
    pub four_cc: i32
}

impl Lump {
    pub fn read(reader: &mut Read) -> Result<Lump, Error> {
        let file_offset = reader.read_i32::<LittleEndian>();
        if file_offset.is_err() {
            return Err(file_offset.err().unwrap());
        }
        let file_length = reader.read_i32::<LittleEndian>();
        if file_length.is_err() {
            return Err(file_length.err().unwrap());
        }
        let version = reader.read_i32::<LittleEndian>();
        if version.is_err() {
            return Err(version.err().unwrap());
        }
        let four_cc = reader.read_i32::<LittleEndian>();
        if four_cc.is_err() {
            return Err(four_cc.err().unwrap());
        }

        return Ok(Lump {
            file_offset: file_offset.unwrap(),
            file_length: file_length.unwrap(),
            version: version.unwrap(),
            four_cc: four_cc.unwrap()
        });
    }
}
