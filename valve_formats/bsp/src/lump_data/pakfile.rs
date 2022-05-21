use zip::ZipArchive;
use std::{io::Cursor, collections::HashMap};
use crate::RawDataRead;

pub struct PakFile {
  archive: ZipArchive<Cursor<Box<[u8]>>>,
  depth_file_names: HashMap<String, String>,
}

impl PakFile {
  pub(crate) fn new(data: Box<[u8]>) -> Self {
    let archive = ZipArchive::new(Cursor::new(data)).unwrap();
    let file_names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    let mut depth_file_names = HashMap::<String, String>::new();
    for file_name in file_names {
      let depth_index = file_name.rfind("_depth_");
      if let Some(depth_index) = depth_index {
        depth_file_names.insert(file_name[..depth_index].to_string(), file_name.to_string() + ".vmt");
      }
    }
    Self {
      archive,
      depth_file_names,
    }
  }

  pub fn contains_entry(&mut self, name: &str) -> bool {
    let lower_case_name = name.to_lowercase();
    let mut actual_name = Option::<String>::None;
    for zip_file_name in self.archive.file_names() {
      if zip_file_name == lower_case_name {
        actual_name = Some(zip_file_name.to_string());
      }
    }
    for (file_name_without_depth, zip_file_name) in &self.depth_file_names {
      if file_name_without_depth.as_str() == lower_case_name {
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
