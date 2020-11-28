pub struct PackageEntry {
  /// File name of this entry
  pub file_name: String,

  /// The name of the directory this file is in.
  /// '/' is always used as a directory separator in Valve's implementation.
  /// Directory names are also always lower cased in Valve's implementation.
  pub directory_name: String,

  /// The file extension
  /// If the file has no extension, this is an empty string
  pub type_name: String,

  /// The CRC32 checksum of this entry
  pub crc32: u32,

  /// the length in bytes
  pub len: u32,

  /// The offset in the package
  pub offset: u32,

  /// Which archive this entry is in
  pub archive_index: u16,

  /// The preloaded bytes
  pub small_data: Vec<u8>
}

impl PackageEntry {
  pub fn total_len(&self) -> u32 {
    self.len + self.small_data.len() as u32
  }

  pub fn full_file_name(&self) -> String {
    if self.type_name == " " {
      self.file_name.clone()
    } else {
      self.file_name.clone() + "." + &self.type_name
    }
  }

  pub fn full_path(&self) -> String {
    if self.directory_name == " " {
      return self.full_file_name();
    }
    self.directory_name.clone() + &self.full_file_name()
  }
}

impl ToString for PackageEntry {
  fn to_string(&self) -> String {
    format!("{} crc={:x} metadatasz={} fnumber={}, ofs={:x} sz={}", self.full_path(), self.crc32, self.small_data.len(), self.archive_index, self.offset, self.len)
  }
}
