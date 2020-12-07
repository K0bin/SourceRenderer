use zip::ZipArchive;
use std::io::Cursor;
use crate::RawDataRead;

pub struct PakFile {
  archive: ZipArchive<Cursor<Box<[u8]>>>
}

impl PakFile {
  pub(crate) fn new(data: Box<[u8]>) -> Self {
    let archive = ZipArchive::new(Cursor::new(data)).unwrap();
    Self {
      archive
    }
  }

  pub fn read_entry(&mut self, name: &str) -> Option<Box<[u8]>> {
    let mut entry = self.archive.by_name(name).ok()?;
    let size = entry.size();
    entry.read_data(size as usize).ok()
  }
}
