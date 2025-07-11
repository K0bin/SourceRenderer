use std::{
    future::Future,
    io::{Result as IOResult, SeekFrom},
};

use futures_lite::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

use crate::StringReadError;

pub trait StringReadAsync {
    fn read_null_terminated_string(
        &mut self,
    ) -> impl Future<Output = Result<String, StringReadError>>;
    fn read_fixed_length_null_terminated_string(
        &mut self,
        length: u32,
    ) -> impl Future<Output = Result<String, StringReadError>>;
}

impl<T: AsyncRead + ?Sized + Unpin> StringReadAsync for T {
    async fn read_null_terminated_string(&mut self) -> Result<String, StringReadError> {
        let mut buffer = Vec::<u8>::new();
        loop {
            let char = self.read_u8().await.map_err(StringReadError::IOError)?;
            if char == 0 {
                break;
            }
            buffer.push(char);
        }
        String::from_utf8(buffer).map_err(StringReadError::StringConstructionError)
    }

    async fn read_fixed_length_null_terminated_string(
        &mut self,
        length: u32,
    ) -> Result<String, StringReadError> {
        let mut buffer = Vec::<u8>::with_capacity(length as usize);
        unsafe {
            buffer.set_len(length as usize);
        }
        self.read_exact(&mut buffer)
            .await
            .map_err(StringReadError::IOError)?;
        for i in 0..buffer.len() {
            let char = buffer[i];
            if char == 0 {
                buffer.resize(i, 0u8);
                break;
            }
        }
        String::from_utf8(buffer).map_err(StringReadError::StringConstructionError)
    }
}

pub trait RawDataReadAsync {
    fn read_data(&mut self, max_len: usize) -> impl Future<Output = IOResult<Box<[u8]>>>;
    fn read_data_exact(&mut self, len: usize) -> impl Future<Output = IOResult<Box<[u8]>>>;
    fn read_data_padded(&mut self, len: usize) -> impl Future<Output = IOResult<Box<[u8]>>>;
}

impl<T: AsyncRead + ?Sized + Unpin> RawDataReadAsync for T {
    async fn read_data_exact(&mut self, len: usize) -> IOResult<Box<[u8]>> {
        let mut buffer = Vec::with_capacity(len);
        unsafe {
            buffer.set_len(len);
        }
        self.read_exact(&mut buffer).await?;
        Ok(buffer.into_boxed_slice())
    }

    async fn read_data(&mut self, len: usize) -> IOResult<Box<[u8]>> {
        let mut buffer = Vec::with_capacity(len);
        unsafe {
            buffer.set_len(len);
        }

        let mut read_offset = 0;
        let mut bytes_read = usize::MAX;
        while read_offset < buffer.len() && bytes_read != 0 {
            bytes_read = self.read(&mut buffer[read_offset..]).await?;
            read_offset += bytes_read;
        }

        buffer.resize(read_offset, 0u8);
        Ok(buffer.into_boxed_slice())
    }

    async fn read_data_padded(&mut self, len: usize) -> IOResult<Box<[u8]>> {
        let mut buffer = Vec::with_capacity(len);
        buffer.resize(len, 0u8);

        let mut read_offset = 0;
        let mut bytes_read = usize::MAX;
        while read_offset < buffer.len() && bytes_read != 0 {
            bytes_read = self.read(&mut buffer[read_offset..]).await?;
            read_offset += bytes_read;
        }

        Ok(buffer.into_boxed_slice())
    }
}

pub trait ReadEntireSeekableFileAsync {
    fn read_seekable_to_end(&mut self) -> impl Future<Output = IOResult<Box<[u8]>>>;
}

// The standard library read_to_end function does a lot of small reads because it can't rely on Seek.
impl<T: RawDataReadAsync + AsyncSeek + ?Sized + Unpin> ReadEntireSeekableFileAsync for T {
    async fn read_seekable_to_end(&mut self) -> IOResult<Box<[u8]>> {
        let len = self.seek(SeekFrom::End(0)).await? as usize;
        let _ = self.seek(SeekFrom::Start(0)).await?;
        self.read_data_exact(len).await
    }
}

pub trait PrimitiveReadAsync {
    fn read_u8(&mut self) -> impl Future<Output = IOResult<u8>>;
    fn read_u16(&mut self) -> impl Future<Output = IOResult<u16>>;
    fn read_u32(&mut self) -> impl Future<Output = IOResult<u32>>;
    fn read_u64(&mut self) -> impl Future<Output = IOResult<u64>>;
    fn read_i8(&mut self) -> impl Future<Output = IOResult<i8>>;
    fn read_i16(&mut self) -> impl Future<Output = IOResult<i16>>;
    fn read_i32(&mut self) -> impl Future<Output = IOResult<i32>>;
    fn read_i64(&mut self) -> impl Future<Output = IOResult<i64>>;
    fn read_f32(&mut self) -> impl Future<Output = IOResult<f32>>;
    fn read_f64(&mut self) -> impl Future<Output = IOResult<f64>>;
}

impl<T: AsyncRead + ?Sized + Unpin> PrimitiveReadAsync for T {
    async fn read_u8(&mut self) -> IOResult<u8> {
        let mut buffer = [0u8; 1];
        self.read_exact(&mut buffer).await?;
        Ok(u8::from_le_bytes(buffer))
    }

    async fn read_u16(&mut self) -> IOResult<u16> {
        let mut buffer = [0u8; 2];
        self.read_exact(&mut buffer).await?;
        Ok(u16::from_le_bytes(buffer))
    }

    async fn read_u32(&mut self) -> IOResult<u32> {
        let mut buffer = [0u8; 4];
        self.read_exact(&mut buffer).await?;
        Ok(u32::from_le_bytes(buffer))
    }

    async fn read_u64(&mut self) -> IOResult<u64> {
        let mut buffer = [0u8; 8];
        self.read_exact(&mut buffer).await?;
        Ok(u64::from_le_bytes(buffer))
    }

    async fn read_i8(&mut self) -> IOResult<i8> {
        let mut buffer = [0u8; 1];
        self.read_exact(&mut buffer).await?;
        Ok(i8::from_le_bytes(buffer))
    }

    async fn read_i16(&mut self) -> IOResult<i16> {
        let mut buffer = [0u8; 2];
        self.read_exact(&mut buffer).await?;
        Ok(i16::from_le_bytes(buffer))
    }

    async fn read_i32(&mut self) -> IOResult<i32> {
        let mut buffer = [0u8; 4];
        self.read_exact(&mut buffer).await?;
        Ok(i32::from_le_bytes(buffer))
    }

    async fn read_i64(&mut self) -> IOResult<i64> {
        let mut buffer = [0u8; 8];
        self.read_exact(&mut buffer).await?;
        Ok(i64::from_le_bytes(buffer))
    }

    async fn read_f32(&mut self) -> IOResult<f32> {
        let mut buffer = [0u8; 4];
        self.read_exact(&mut buffer).await?;
        Ok(f32::from_le_bytes(buffer))
    }

    async fn read_f64(&mut self) -> IOResult<f64> {
        let mut buffer = [0u8; 8];
        self.read_exact(&mut buffer).await?;
        Ok(f64::from_le_bytes(buffer))
    }
}
