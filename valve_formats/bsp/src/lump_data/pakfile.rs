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

  pub fn contains_entry(&mut self, name: &str) -> bool {
    let lower_case_name = name.to_lowercase();
    let mut actual_name = Option::<String>::None;
    for zip_file_name in self.archive.file_names() {
      if zip_file_name.to_lowercase() == lower_case_name {
        actual_name = Some(zip_file_name.to_string());
      }
    }
    actual_name.is_some()
  }

  pub fn read_entry(&mut self, name: &str) -> Option<Box<[u8]>> {
    let lower_case_name = name.to_lowercase();
    let mut actual_name = Option::<String>::None;
    for zip_file_name in self.archive.file_names() {
      if zip_file_name.to_lowercase() == lower_case_name {
        actual_name = Some(zip_file_name.to_string());
      }
    }
    actual_name.and_then(|name| {
      let mut entry = self.archive.by_name(&name).ok()?;
      let size = entry.size();
      entry.read_data(size as usize).ok()
    })
  }
}
