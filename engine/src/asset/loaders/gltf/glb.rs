use std::io::{ErrorKind, Result as IOResult, Error as IOError};

use bevy_tasks::futures_lite::io::{AsyncRead, AsyncReadExt};

use io_util::PrimitiveReadAsync;

pub struct GlbHeader {
    magic: u32,
    version: u32,
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
            magic,
            version,
            length,
        })
    }

    pub fn size() -> u64 {
        12
    }
}

pub struct GlbChunkHeader {
    pub length: u32,
    chunk_type: u32,
}

impl GlbChunkHeader {
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> IOResult<Self> {
        let length = reader.read_u32().await?;
        let chunk_type = reader.read_u32().await?;

        if chunk_type != 0x4E4F534A && chunk_type != 0x004E4942 {
            // "JSON" || "BIN"
            return Err(IOError::new(ErrorKind::Other, "Invalid chunk type"));
        }

        Ok(Self { length, chunk_type })
    }

    pub fn size() -> u64 {
        8
    }
}

pub async fn read_chunk<R: AsyncRead + Unpin>(reader: &mut R) -> IOResult<Vec<u8>> {
    let header = GlbChunkHeader::read(reader).await?;
    let mut data = Vec::with_capacity(header.length as usize);
    unsafe {
        data.set_len(header.length as usize);
    }
    reader.read_exact(&mut data).await?;
    Ok(data)
}
