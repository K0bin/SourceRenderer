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
        let file_offset = reader.read_i32::<LittleEndian>()?;
        let file_length = reader.read_i32::<LittleEndian>()?;
        let version = reader.read_i32::<LittleEndian>()?;
        let four_cc = reader.read_i32::<LittleEndian>()?;

        return Ok(Lump {
            file_offset,
            file_length,
            version,
            four_cc
        });
    }
}
