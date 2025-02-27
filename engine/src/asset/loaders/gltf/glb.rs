use std::io::{ErrorKind, Result as IOResult, Error as IOError};

use bevy_tasks::futures_lite::io::AsyncRead;

use io_util::PrimitiveReadAsync;

pub struct GlbHeader {
    _magic: u32,
    _version: u32,
    pub length: u32,
}

impl GlbHeader {
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> IOResult<Self> {
        let magic = reader.read_u32().await?;
        let version = reader.read_u32().await?;
        let length = reader.read_u32().await?;

        if magic != 0x46546c67 {
            // glTF
            return Err(IOError::new(ErrorKind::Other, "Invalid format"));
        }

        if version != 2 {
            return Err(IOError::new(ErrorKind::Other, "Invalid version"));
        }

        Ok(Self {
            _magic: magic,
            _version: version,
            length,
        })
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn size() -> u64 {
        12
    }
}

pub struct GlbChunkHeader {
    pub length: u32,
    _chunk_type: u32,
}

impl GlbChunkHeader {
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> IOResult<Self> {
        let length = reader.read_u32().await?;
        let chunk_type = reader.read_u32().await?;

        if chunk_type != 0x4E4F534A && chunk_type != 0x004E4942 {
            // "JSON" || "BIN"
            return Err(IOError::new(ErrorKind::Other, "Invalid chunk type"));
        }

        Ok(Self { length, _chunk_type: chunk_type })
    }

    pub fn size() -> u64 {
        8
    }
}
