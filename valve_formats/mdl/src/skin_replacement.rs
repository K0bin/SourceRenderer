use std::collections::HashMap;
use std::io::{Cursor, ErrorKind, Read, Result as IOResult};

use crate::PrimitiveRead;

pub struct SkinReplacementTableEntry {
    pub main_body: u16,
    pub trimming: u16,
}

pub struct SkinReplacementTable {
    table: HashMap<u16, SkinReplacementTableEntry>,
}

impl SkinReplacementTable {
    pub fn read(read: &mut dyn Read, skin_families_count: i32, skins_count: i32) -> IOResult<Self> {
        let len = (skin_families_count * skins_count * 2) as usize;
        let mut data = Vec::with_capacity(len);
        unsafe {
            data.set_len(len);
        }
        read.read_exact(&mut data)?;

        let mut table = HashMap::<u16, SkinReplacementTableEntry>::new();
        let mut i = 0;
        let mut cursor = Cursor::new(&data);
        loop {
            let main_body_res = cursor.read_u16();
            if let Err(e) = main_body_res {
                if e.kind() == ErrorKind::UnexpectedEof {
                    break;
                } else {
                    return Err(e);
                }
            }
            let main_body = main_body_res.unwrap();
            let trimming = cursor.read_u16()?;
            table.insert(
                i,
                SkinReplacementTableEntry {
                    main_body,
                    trimming,
                },
            );
            i += 1;
        }

        Ok(Self { table })
    }
}
