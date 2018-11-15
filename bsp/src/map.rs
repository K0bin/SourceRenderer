use lump::{Lump, read_lump};

use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

const LUMP_COUNT: usize = 64;

pub struct MapHeader {
    pub identifier: i32,
    pub version: i32,
    pub lumps: [Lump; LUMP_COUNT],
}

pub fn read_map(reader: &mut Read) -> Result<MapHeader, Error> {
    let identifier = reader.read_i32::<LittleEndian>();
    if identifier.is_err() {
        return Err(identifier.err().unwrap());
    }
    let version = reader.read_i32::<LittleEndian>();
    if version.is_err() {
        return Err(version.err().unwrap());
    }
    let mut lumps: [Lump; LUMP_COUNT] = [
        Lump {
            file_offset: 0,
            file_length: 0,
            version: 0,
            four_cc: 0
        };
        LUMP_COUNT
    ];
    for i in 0..LUMP_COUNT {
        let lump = read_lump(reader);
        if lump.is_err() {
            return Err(lump.err().unwrap());
        }
        lumps[i] = lump.unwrap();
    }
    return Ok(MapHeader {
        identifier: identifier.unwrap(),
        version: version.unwrap(),
        lumps: lumps
    });
}
