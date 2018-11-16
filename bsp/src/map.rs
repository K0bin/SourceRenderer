use map_header::{MapHeader};
use std::io::{Read, Error, ErrorKind, Seek, SeekFrom, BufReader};
use std::fs::File;
use lump_data::{LumpData, LumpType, Brush, read_lump_data};
use std::ops::DerefMut;
use std::boxed::{Box};

pub struct Map {
    pub name: String,
    header: MapHeader,
    reader: Box<BufReader<File>>
}

impl Map {
    pub fn read(name: String, mut reader: Box<BufReader<File>>) -> Result<Map, Error> {
        let header = MapHeader::read(&mut reader);
        if (header.is_err()) {
            return Err(header.err().unwrap());
        }
        return Ok(Map {
            name: name,
            header: header.unwrap(),
            reader: reader
        });
    }

    pub fn read_lump_data(&mut self, lump_type: LumpType) -> Result<LumpData, Error> {
        let index = lump_type as usize;
        let lump = self.header.lumps[index];
        let seek_result = self.reader.seek(SeekFrom::Start(lump.file_offset as u64));
        return seek_result.and_then(|_result| read_lump_data(&mut self.reader, lump_type, lump.file_length, self.header.version));
    }
}
