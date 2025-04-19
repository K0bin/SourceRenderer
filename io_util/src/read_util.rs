use std::io::{Error as IOError, Read, Result as IOResult, Seek, SeekFrom};
use std::string::FromUtf8Error;

#[derive(Debug)]
pub enum StringReadError {
  IOError(IOError),
  StringConstructionError(FromUtf8Error)
}

pub trait StringRead {
  fn read_null_terminated_string(&mut self) -> Result<String, StringReadError>;
  fn read_fixed_length_null_terminated_string(&mut self, length: u32) -> Result<String, StringReadError>;
}

impl<T: Read + ?Sized> StringRead for T {
  fn read_null_terminated_string(&mut self) -> Result<String, StringReadError> {
    let mut buffer = Vec::<u8>::new();
    loop {
      let char = self.read_u8().map_err(StringReadError::IOError)?;
      if char == 0 {
        break;
      }
      buffer.push(char);
    }
    String::from_utf8(buffer).map_err(StringReadError::StringConstructionError)
  }

  fn read_fixed_length_null_terminated_string(&mut self, length: u32) -> Result<String, StringReadError> {
    let mut buffer = Vec::<u8>::with_capacity(length as usize);
    unsafe { buffer.set_len(length as usize); }
    self.read_exact(&mut buffer).map_err(StringReadError::IOError)?;
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

pub trait RawDataRead {
  fn read_data(&mut self, len: usize) -> IOResult<Box<[u8]>>;
  fn read_data_exact(&mut self, len: usize) -> IOResult<Box<[u8]>>;
  fn read_data_padded(&mut self, len: usize) -> IOResult<Box<[u8]>>;
}

impl<T: Read + ?Sized> RawDataRead for T {
  fn read_data_exact(&mut self, len: usize) -> IOResult<Box<[u8]>> {
    let mut buffer = Vec::with_capacity(len);
    unsafe { buffer.set_len(len); }
    self.read_exact(&mut buffer)?;
    Ok(buffer.into_boxed_slice())
  }

  fn read_data(&mut self, len: usize) -> IOResult<Box<[u8]>> {
    let mut buffer = Vec::with_capacity(len);
    unsafe { buffer.set_len(len); }

    let mut read_offset = 0;
    let mut bytes_read = usize::MAX;
    while read_offset < buffer.len() && bytes_read != 0 {
        bytes_read = self.read(&mut buffer[read_offset..])?;
        read_offset += bytes_read;
    }

    buffer.resize(read_offset, 0u8);
    Ok(buffer.into_boxed_slice())
  }

  fn read_data_padded(&mut self, len: usize) -> IOResult<Box<[u8]>> {
    let mut buffer = Vec::with_capacity(len);
    buffer.resize(len, 0u8);

    let mut read_offset = 0;
    let mut bytes_read = usize::MAX;
    while read_offset < buffer.len() && bytes_read != 0 {
        bytes_read = self.read(&mut buffer[read_offset..])?;
        read_offset += bytes_read;
    }

    Ok(buffer.into_boxed_slice())
  }
}

pub trait ReadEntireSeekableFile {
  fn read_seekable_to_end(&mut self) -> IOResult<Box<[u8]>>;
}

// The standard library read_to_end function does a lot of small reads because it can't rely on Seek.
impl<T: RawDataRead + Seek + ?Sized + Unpin> ReadEntireSeekableFile for T {
  fn read_seekable_to_end(&mut self) -> IOResult<Box<[u8]>> {
    let len = self.seek(SeekFrom::End(0))? as usize;
    let _ = self.seek(SeekFrom::Start(0))?;
    self.read_data_exact(len)
  }
}

pub trait PrimitiveRead {
  fn read_u8(&mut self) -> IOResult<u8>;
  fn read_u16(&mut self) -> IOResult<u16>;
  fn read_u32(&mut self) -> IOResult<u32>;
  fn read_u64(&mut self) -> IOResult<u64>;
  fn read_i8(&mut self) -> IOResult<i8>;
  fn read_i16(&mut self) -> IOResult<i16>;
  fn read_i32(&mut self) -> IOResult<i32>;
  fn read_i64(&mut self) -> IOResult<i64>;
  fn read_f32(&mut self) -> IOResult<f32>;
  fn read_f64(&mut self) -> IOResult<f64>;
}

impl<T: Read + ?Sized> PrimitiveRead for T {
  fn read_u8(&mut self) -> IOResult<u8> {
    let mut buffer = [0u8; 1];
    self.read_exact(&mut buffer)?;
    Ok(u8::from_le_bytes(buffer))
  }

  fn read_u16(&mut self) -> IOResult<u16> {
    let mut buffer = [0u8; 2];
    self.read_exact(&mut buffer)?;
    Ok(u16::from_le_bytes(buffer))
  }

  fn read_u32(&mut self) -> IOResult<u32> {
    let mut buffer = [0u8; 4];
    self.read_exact(&mut buffer)?;
    Ok(u32::from_le_bytes(buffer))
  }

  fn read_u64(&mut self) -> IOResult<u64> {
    let mut buffer = [0u8; 8];
    self.read_exact(&mut buffer)?;
    Ok(u64::from_le_bytes(buffer))
  }

  fn read_i8(&mut self) -> IOResult<i8> {
    let mut buffer = [0u8; 1];
    self.read_exact(&mut buffer)?;
    Ok(i8::from_le_bytes(buffer))
  }

  fn read_i16(&mut self) -> IOResult<i16> {
    let mut buffer = [0u8; 2];
    self.read_exact(&mut buffer)?;
    Ok(i16::from_le_bytes(buffer))
  }

  fn read_i32(&mut self) -> IOResult<i32> {
    let mut buffer = [0u8; 4];
    self.read_exact(&mut buffer)?;
    Ok(i32::from_le_bytes(buffer))
  }

  fn read_i64(&mut self) -> IOResult<i64> {
    let mut buffer = [0u8; 8];
    self.read_exact(&mut buffer)?;
    Ok(i64::from_le_bytes(buffer))
  }

  fn read_f32(&mut self) -> IOResult<f32> {
    let mut buffer = [0u8; 4];
    self.read_exact(&mut buffer)?;
    Ok(f32::from_le_bytes(buffer))
  }

  fn read_f64(&mut self) -> IOResult<f64> {
    let mut buffer = [0u8; 8];
    self.read_exact(&mut buffer)?;
    Ok(f64::from_le_bytes(buffer))
  }
}
