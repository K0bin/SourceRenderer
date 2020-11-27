use std::io::Read;
use std::io::Result as IOResult;

pub(crate) fn read_u8(read: &mut dyn Read) -> IOResult<u8> {
  let mut buffer = [0u8; 1];
  read.read(&mut buffer)?;
  Ok(u8::from_le_bytes(buffer))
}

pub(crate) fn read_u16(read: &mut dyn Read) -> IOResult<u16> {
  let mut buffer = [0u8; 2];
  read.read(&mut buffer)?;
  Ok(u16::from_le_bytes(buffer))
}

pub(crate) fn read_u32(read: &mut dyn Read) -> IOResult<u32> {
  let mut buffer = [0u8; 4];
  read.read(&mut buffer)?;
  Ok(u32::from_le_bytes(buffer))
}

pub(crate) fn read_u64(read: &mut dyn Read) -> IOResult<u64> {
  let mut buffer = [0u8; 8];
  read.read(&mut buffer)?;
  Ok(u64::from_le_bytes(buffer))
}

pub(crate) fn read_i8(read: &mut dyn Read) -> IOResult<i8> {
  let mut buffer = [0u8; 1];
  read.read(&mut buffer)?;
  Ok(i8::from_le_bytes(buffer))
}

pub(crate) fn read_i16(read: &mut dyn Read) -> IOResult<i16> {
  let mut buffer = [0u8; 2];
  read.read(&mut buffer)?;
  Ok(i16::from_le_bytes(buffer))
}

pub(crate) fn read_i32(read: &mut dyn Read) -> IOResult<i32> {
  let mut buffer = [0u8; 4];
  read.read(&mut buffer)?;
  Ok(i32::from_le_bytes(buffer))
}

pub(crate) fn read_i64(read: &mut dyn Read) -> IOResult<i64> {
  let mut buffer = [0u8; 8];
  read.read(&mut buffer)?;
  Ok(i64::from_le_bytes(buffer))
}

pub(crate) fn read_f32(read: &mut dyn Read) -> IOResult<f32> {
  let mut buffer = [0u8; 4];
  read.read(&mut buffer)?;
  Ok(f32::from_le_bytes(buffer))
}

pub(crate) fn read_f64(read: &mut dyn Read) -> IOResult<f64> {
  let mut buffer = [0u8; 8];
  read.read(&mut buffer)?;
  Ok(f64::from_le_bytes(buffer))
}
