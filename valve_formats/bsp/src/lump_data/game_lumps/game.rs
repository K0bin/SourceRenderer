use std::io::{Read, Result as IOResult, Seek, SeekFrom, Cursor, Error as IOError, ErrorKind};
use crate::lump_data::{LumpData, LumpType};
use crate::read_util::PrimitiveRead;
use std::ffi::CString;
use crate::lump_data::game_lumps::StaticPropDict;

pub struct GameLumps {
  game_lumps: Box<[GameLump]>
}

impl GameLumps {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let lump_count = read.read_i32()?;
    let mut game_lumps = Vec::<GameLump>::new();
    for _ in 0..lump_count {
      let game_lump = GameLump::read(read)?;
      game_lumps.push(game_lump);
    }
    Ok(Self {
      game_lumps: game_lumps.into_boxed_slice()
    })
  }

  pub(crate) fn read_static_prop_dict<R: Read + Seek>(&self, mut read: &mut R) -> IOResult<StaticPropDict> {
    for lump in self.game_lumps.as_ref() {
      if lump.id == StaticPropDict::id() {
        read.seek(SeekFrom::Start(lump.file_offset as u64));
        let mut data = Vec::with_capacity(lump.file_length as usize);
        unsafe  {
          data.set_len(lump.file_length as usize);
        }
        read.read_exact(&mut data);
        let mut cursor = Cursor::new(data);
        let static_props = StaticPropDict::read(&mut cursor, lump.version)?;
        return Ok(static_props);
      }
    }

    Err(IOError::new(ErrorKind::Other, "Game lump not found"))
  }
}

pub struct GameLump {
  pub id: u32,
  pub flags: u16,
  pub version: u16,
  pub file_offset: i32,
  pub file_length: i32
}

impl GameLump {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Self> {
    let id = read.read_u32()?;
    let flags = read.read_u16()?;
    let version = read.read_u16()?;
    let file_offset = read.read_i32()?;
    let file_length = read.read_i32()?;
    println!("entity id: {:?}", id);
    Ok(Self {
      id,
      flags,
      version,
      file_offset,
      file_length
    })
  }
}
