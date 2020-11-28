use std::io::{Read, Seek, SeekFrom, Result as IOResult};
use std::cmp::min;

pub struct BufReader<T> {
  inner: T,
  buffer: Box<[u8]>,
  buffer_position: usize,
  buffer_len: usize
}

impl<T> BufReader<T> {
  pub fn new(inner: T) -> Self {
    let buffer_size = 8192;
    let mut buffer = Vec::<u8>::with_capacity(buffer_size);
    unsafe {
      buffer.set_len(buffer_size);
    }
    Self {
      inner,
      buffer: buffer.into_boxed_slice(),
      buffer_position: 0,
      buffer_len: 0
    }
  }
}

impl<T: Read> Read for BufReader<T> {
  fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
    let mut rem = buf.len();

    let initially_buffered = min(self.buffer_len - self.buffer_position, buf.len());
    if initially_buffered > 0 {
      &mut buf[..initially_buffered].copy_from_slice(&self.buffer[self.buffer_position .. self.buffer_position + initially_buffered]);
      self.buffer_position += initially_buffered;
      rem -= initially_buffered;
    }

    if rem == 0 {
      Ok(buf.len())
    } else if self.buffer_len != 0 && self.buffer_len < self.buffer.len() {
      // we're at the end of the file
      Ok(initially_buffered)
    } else if rem >= self.buffer.len() {
      // remaining wouldn't fit into the buffer anyway
      self.buffer_position = 0;
      self.buffer_len = 0;
      let len = buf.len();
      let read = self.inner.read(&mut buf[initially_buffered .. len])?;
      Ok(read + initially_buffered)
    } else {
      // fill the buffer and copy the remainder
      let read = self.inner.read(&mut self.buffer)?;
      self.buffer_len = read;
      &mut buf[initially_buffered..].copy_from_slice(&self.buffer[..rem]);
      self.buffer_position = rem;
      Ok(min(read + initially_buffered, buf.len()))
    }
  }
}

impl<T: Seek> Seek for BufReader<T> {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    let reader_pos = self.inner.seek(SeekFrom::Current(0))?;
    let buffer_start_pos = reader_pos - self.buffer_len as u64;
    let current_pos = buffer_start_pos + self.buffer_position as u64;
    let target_pos: u64 = match pos {
      SeekFrom::Current(pos) => (current_pos as i64 + pos) as u64,
      SeekFrom::Start(pos) => pos,
      SeekFrom::End(pos) => {
        self.buffer_position = 0;
        self.buffer_len = 0;
        return self.inner.seek(SeekFrom::End(pos));
      }
    };

    if target_pos == current_pos {
      // User just wants to know the current position
      return Ok(current_pos);
    }

    return if target_pos < buffer_start_pos + self.buffer_len as u64 {
      // we can seek within the buffered data
      self.buffer_position += (target_pos - buffer_start_pos) as usize;
      Ok(target_pos)
    } else {
      // invalidate buffered data and seek in the data source
      self.buffer_position = 0;
      self.buffer_len = 0;
      self.inner.seek(SeekFrom::Start(target_pos))
    }
  }
}
