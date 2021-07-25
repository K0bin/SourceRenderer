use std::io::{Read, BufReader, Seek, Error as IOError, Result as IOResult, SeekFrom};
use package_entry::PackageEntry;
use std::collections::HashMap;
use archive_md5_section_entry::ArchiveMD5SectionEntry;
use read_util::{PrimitiveRead, StringRead, StringReadError, RawDataRead};
use crc::crc32;
use utilities::AsnKeyParser;
use rsa::{BigUint, PaddingScheme, Hash, PublicKey};
use rand::rngs::OsRng;
use std::sync::Mutex;

#[derive(Debug)]
pub enum PackageError {
  IOError(IOError),
  FileError(String)
}

pub struct Package<R>
  where R : Read + Seek {
  reader: Mutex<R>,
  is_dir_vpk: bool,
  header_size: u32,

  /// The file name
  file_name: String,

  /// The VPK version
  version: u32,

  /// The size in bytes of the directory tree.
  tree_size: u32,
  /// How many bytes of file content are stored in this VPK file (0 in CSGO)
  file_data_section_size: u32,

  /// The size in bytes of the section containing MD5 checksums for external archive content
  archive_md5_section_size: u32,

  /// The size in bytes of the section containing MD5 checksums for content in this file
  other_md5_section_size: u32,

  /// The size in bytes of the section containing the public key and signature
  signature_section_size: u32,

  /// The MD5 checksum of the file tree
  tree_checksum: [u8; 16],

  /// The MD5 checksum of the archive MD5 checksum section entries.
  archive_md5_entries_checksum: [u8; 16],

  /// The MD5 checksum of the complete package until the signature structure
  whole_file_checksum: [u8; 16],

  /// The public key
  public_key: Box<[u8]>,

  /// The signature
  signature: Box<[u8]>,

  /// The package entries
  entries: HashMap<String, Vec<PackageEntry>>,

  /// The archive MD5 checksum section entries. Also known as cache line hashes.
  archive_md5_entries: Vec<ArchiveMD5SectionEntry>,

  archive_files: Mutex<HashMap<u16, BufReader<R>>>
}

pub const MAGIC: u32 = 0x55AA1234;

/// Always '/' as per Valve's vpk implementation.
pub const DIRECTORY_SEPARATOR: &str = "/";

impl<R> Package<R>
  where R : Read + Seek {
  /// Gets the File Name
  pub fn file_name(&self) -> &str {
    self.file_name.as_str()
  }

  /// Gets the VPK version.
  pub fn version(&self) -> u32 {
    self.version
  }

  /// Gets the size in bytes of the directory tree.
  pub fn tree_size(&self) -> u32 {
    self.tree_size
  }

  /// Gets how many bytes of file content are stored in this VPK file (0 in CSGO).
  pub fn file_data_section_size(&self) -> u32 {
    self.file_data_section_size
  }

  /// Gets the size in bytes of the section containing MD5 checksums for external archive content.
  pub fn archive_md5_section_size(&self) -> u32 {
    self.archive_md5_section_size
  }

  /// Gets the size in bytes of the section containing MD5 checksums for content in this file.
  pub fn other_md5_section_size(&self) -> u32 {
    self.other_md5_section_size
  }

  /// Gets the size in bytes of the section containing MD5 checksums for content in this file.
  pub fn signature_section_size(&self) -> u32 {
    self.signature_section_size
  }

  /// Gets the MD5 checksum of the file tree.
  pub fn tree_checksum(&self) -> &[u8] {
    &self.tree_checksum
  }

  /// Gets the MD5 checksum of the archive MD5 checksum section entries.
  pub fn archive_md5_entries_checksum(&self) -> &[u8] {
    &self.archive_md5_entries_checksum
  }

  /// Gets the MD5 checksum of the complete package until the signature structure.
  pub fn whole_file_checksum(&self) -> &[u8] {
    &self.whole_file_checksum
  }

  /// Gets the public key.
  pub fn public_key(&self) -> &[u8] {
    &self.public_key
  }

  /// Gets the signature.
  pub fn signature(&self) -> &[u8] {
    &self.signature
  }

  /// Gets the package entries.
  pub fn entries(&self) -> &HashMap<String, Vec<PackageEntry>> {
    &self.entries
  }

  /// Gets the archive MD5 checksum section entries. Also known as cache line hashes.
  pub fn archive_md5_entries(&self) -> &Vec<ArchiveMD5SectionEntry> {
    &self.archive_md5_entries
  }

  pub fn sanitize_file_name(file_name: &str) -> (String, bool) {
    let lower_file_name = file_name.to_lowercase();
    let mut file_name_str = lower_file_name.as_str();
    if file_name_str.ends_with(".vpk") {
      file_name_str = &file_name[0 .. file_name_str.len() - 4];
    }

    if file_name_str.ends_with("_dir") {
      return (file_name_str[0 .. file_name_str.len() - 4].to_string(), true);
    }

    (file_name_str.to_string(), false)
  }

  pub fn read<F: 'static + Send + Sync + Fn(&str) -> IOResult<R>>(file_name: &str, mut input: R, open_file_callback: F) -> Result<Self, PackageError> {
    let (file_name, is_dir_vpk) = Self::sanitize_file_name(file_name);

    if input.read_u32().map_err(PackageError::IOError)? != MAGIC {
      return Err(PackageError::FileError("Given file is not a VPK.".to_string()));
    }

    let version = input.read_u32().map_err(PackageError::IOError)?;
    let tree_size = input.read_u32().map_err(PackageError::IOError)?;

    let (file_data_section_size,
      archive_md5_section_size,
      other_md5_section_size,
      signature_section_size) =
    if version == 1 {
      (0u32, 0u32, 0u32, 0u32)
    } else if version == 2 {
      (
        input.read_u32().map_err(PackageError::IOError)?,
        input.read_u32().map_err(PackageError::IOError)?,
        input.read_u32().map_err(PackageError::IOError)?,
        input.read_u32().map_err(PackageError::IOError)?,
        )
    } else {
      return Err(PackageError::FileError(format!("Bad VPK version: {}", version)));
    };

    let header_size = input.seek(SeekFrom::Current(0)).map_err(PackageError::IOError)? as u32;

    let entries = Self::read_entries(&mut input)?;

    let mut archive_files = HashMap::<u16, BufReader<R>>::new();
    for (_name, entries) in &entries {
      for entry in entries {
        if archive_files.contains_key(&entry.archive_index) {
          continue;
        }

        let file_name = format!("{}_{:03}.vpk", file_name, entry.archive_index);
        let file = (open_file_callback)(&file_name);
        if file.is_err() {
          // apparently broken entries are a thing and supposed to be ignored I guess
          continue;
        }
        archive_files.insert(entry.archive_index, BufReader::new(file.map_err(PackageError::IOError)?));
      }
    }

    let (archive_md5_entries, tree_checksum, archive_md5_entries_checksum, whole_file_checksum, public_key, signature) =
      if version == 2 {
        input.seek(SeekFrom::Current(file_data_section_size as i64)).map_err(PackageError::IOError)?;
        let archive_md5_entries = Self::read_archive_md5_section(&mut input, archive_md5_section_size)?;
        let (tree_checksum, archive_md5_entries_checksum, whole_file_checksum) = Self::read_other_md5_section(&mut input, other_md5_section_size)?;
        let (public_key, signature) = Self::read_signature_section(&mut input, signature_section_size)?;
        (archive_md5_entries, tree_checksum, archive_md5_entries_checksum, whole_file_checksum, public_key, signature)
      } else {
        Default::default()
      };

    Ok(Self {
      reader: Mutex::new(input),
      is_dir_vpk,
      header_size,
      file_name,
      version,
      tree_size,
      file_data_section_size,
      archive_md5_section_size,
      other_md5_section_size,
      signature_section_size,
      tree_checksum,
      archive_md5_entries_checksum,
      whole_file_checksum,
      public_key,
      signature,
      entries,
      archive_md5_entries,
      archive_files: Mutex::new(archive_files)
    })
  }

  /// Searches for a given file entry in the file list.
  pub fn find_entry(&self, file_path: &str) -> Option<&PackageEntry> {
    let file_path = file_path.replace("\\", DIRECTORY_SEPARATOR).to_lowercase();
    let last_separator = file_path.rfind(DIRECTORY_SEPARATOR);
    let (file_name, directory) = if let Some(last_separator) = last_separator {
      (&file_path[last_separator + 1 ..], &file_path[.. last_separator])
    } else {
      (file_path.as_str(), "")
    };
    self.find_entry_in_dir(directory, file_name)
  }

  /// Searches for a given file entry in the file list.
  pub fn find_entry_in_dir(&self, directory: &str, file_name: &str) -> Option<&PackageEntry> {
    let dot = file_name.rfind('.');
    let (file_name, extension) = if let Some(dot) = dot {
      (&file_name[.. dot], &file_name[dot + 1 ..])
    } else {
      (file_name, "")
    };
    self.find_entry_in_dir_with_extension(directory, file_name, extension)
  }

  pub fn find_entry_in_dir_with_extension(&self, directory: &str, file_name: &str, file_extension: &str) -> Option<&PackageEntry> {
    if !self.entries.contains_key(file_extension) {
      return None;
    }

    // We normalize path separators when reading the file list
    // And remove the trailing slash
    let directory_separator_char: char = DIRECTORY_SEPARATOR.parse().unwrap();
    let directory = directory.replace('\\', DIRECTORY_SEPARATOR);
    let mut trimmed_directory = directory.trim_matches(directory_separator_char);

    // If the directory is empty after trimming, set it to a space to match Valve's behaviour
    if trimmed_directory.is_empty() {
      trimmed_directory = " ";
    }

    self.entries[file_extension].iter().find(|x| x.directory_name.as_str() == trimmed_directory && x.file_name.as_str() == file_name)
  }

  pub fn read_entry(&self, entry: &PackageEntry, validate_crc: bool) -> Result<Box<[u8]>, PackageError> {
    let output_size = entry.small_data.len() + entry.len as usize;
    let mut output = Vec::with_capacity(output_size);
    unsafe { output.set_len(output_size); }
    if entry.small_data.len() > 0 {
      output[.. entry.small_data.len()].copy_from_slice(&entry.small_data);
    }

    if entry.len > 0 {
      if entry.archive_index != 0x7FFF {
        if !self.is_dir_vpk {
          return Err(PackageError::FileError("Given VPK is not a _dir, but entry is referencing an external archive.".to_string()));
        }

        let offset = entry.offset;
        let mut files = self.archive_files.lock().unwrap();
        let file = files.get_mut(&entry.archive_index).unwrap();
        file.seek(SeekFrom::Start(offset as u64)).map_err(PackageError::IOError)?;
        file.read_exact(&mut output[entry.small_data.len() .. entry.small_data.len() + entry.len as usize]).map_err(PackageError::IOError)?;
      } else {
        let offset = self.header_size + self.tree_size + entry.offset;
        let mut reader = self.reader.lock().unwrap();
        reader.seek(SeekFrom::Start(offset as u64)).map_err(PackageError::IOError)?;
        reader.read_exact(&mut output[entry.small_data.len() .. entry.small_data.len() + entry.len as usize]).map_err(PackageError::IOError)?;
      }
    }

    if validate_crc && entry.crc32 != crc32::checksum_ieee(&output) {
      return Err(PackageError::FileError("CRC32 mismatch for read data.".to_string()));
    }

    Ok(output.into_boxed_slice())
  }

  fn read_entries(input: &mut R) -> Result<HashMap<String, Vec<PackageEntry>>, PackageError> {
    let mut type_entries = HashMap::<String, Vec<PackageEntry>>::new();

    'types: loop {
      let type_name = input.read_null_terminated_string().map_err(|e| match e {
        StringReadError::IOError(e) => PackageError::IOError(e),
        StringReadError::StringConstructionError(_) => PackageError::FileError("Failed to read type name".to_string())
      })?;
      if type_name.is_empty() {
        break 'types;
      }

      let mut entries = Vec::<PackageEntry>::new();
      'entries: loop {
        let directory_name = input.read_null_terminated_string().map_err(|e| match e {
          StringReadError::IOError(e) => PackageError::IOError(e),
          StringReadError::StringConstructionError(_) => PackageError::FileError("Failed to read type name".to_string())
        })?;
        if directory_name.is_empty() {
          break 'entries;
        }

        'files: loop {
          let file_name = input.read_null_terminated_string().map_err(|e| match e {
            StringReadError::IOError(e) => PackageError::IOError(e),
            StringReadError::StringConstructionError(_) => PackageError::FileError("Failed to read type name".to_string())
          })?;
          if file_name.is_empty() {
            break 'files;
          }

          let crc32 = input.read_u32().map_err(PackageError::IOError)?;
          let small_data_len = input.read_u16().map_err(PackageError::IOError)? as usize;
          let archive_index = input.read_u16().map_err(PackageError::IOError)?;
          let offset = input.read_u32().map_err(PackageError::IOError)?;
          let len = input.read_u32().map_err(PackageError::IOError)?;

          let mut entry = PackageEntry {
            file_name: file_name.to_lowercase(),
            directory_name: directory_name.to_lowercase(),
            type_name: type_name.to_lowercase(),
            crc32,
            small_data: Vec::new().into_boxed_slice(),
            archive_index,
            offset,
            len
          };

          if input.read_u16().map_err(PackageError::IOError)? != 0xFFFF {
            return Err(PackageError::FileError("Invalid terminator.".to_string()));
          }

          if small_data_len > 0 {
            entry.small_data = input.read_data(small_data_len).map_err(PackageError::IOError)?;
          }

          entries.push(entry);
        }
      }

      type_entries.insert(type_name, entries);
    }

    Ok(type_entries)
  }

  /// Verify checksums and signatures provided in the VPK
  pub fn verify_hashes(&self) -> Result<(), PackageError> {
    if self.version != 2 {
      return Err(PackageError::FileError("Only version 2 is supported.".to_string()));
    }

    {
      let mut reader = self.reader.lock().unwrap();
      reader.seek(SeekFrom::Start(0)).map_err(PackageError::IOError)?;
      let mut buffer = reader.read_data((self.header_size + self.tree_size + self.file_data_section_size + self.archive_md5_section_size + 32) as usize).map_err(PackageError::IOError)?;
      let mut hash = md5::compute(&buffer);
      if hash.0 != self.whole_file_checksum {
        return Err(PackageError::FileError(format!("Package checksum mismatch ({:?} != expected {:?}).", &hash, &self.whole_file_checksum)));
      }

      reader.seek(SeekFrom::Start((self.header_size + self.tree_size + self.file_data_section_size) as u64)).map_err(PackageError::IOError)?;
      reader.read_exact(&mut buffer[..self.archive_md5_section_size as usize]).map_err(PackageError::IOError)?;
      hash = md5::compute(&buffer[..self.archive_md5_section_size as usize]);
      if hash.0 != self.whole_file_checksum {
        return Err(PackageError::FileError(format!("Archive MD5 entries checksum mismatch ({:?} != expected {:?}).", &hash, &self.archive_md5_entries_checksum)));
      }

      // TODO: verify archive checksums
    }

    if self.public_key.is_empty() || self.signature.is_empty() {
      return Ok(());
    }

    if !self.is_signature_valid() {
      return Err(PackageError::FileError("VPK signature is not valid.".to_string()));
    }

    Ok(())
  }

  pub fn is_signature_valid(&self) -> bool {
    let mut reader = self.reader.lock().unwrap();
    let seek_res = reader.seek(SeekFrom::Start(0));
    if seek_res.is_err() {
      return false;
    }

    let mut key_parser = AsnKeyParser::new(&self.public_key);
    let parameters_res = key_parser.parse_rsa_public_key();
    if parameters_res.is_err() {
      return false;
    }
    let parameters = parameters_res.unwrap();

    let public_key_res = rsa::RSAPublicKey::new(BigUint::from_bytes_le(&parameters.modulus), BigUint::from_bytes_le(&parameters.exponent));
    if public_key_res.is_err() {
      return false;
    }
    let public_key = public_key_res.unwrap();
    let data_res = reader.read_data((self.header_size + self.tree_size + self.file_data_section_size + self.archive_md5_section_size + self.other_md5_section_size) as usize);
    if data_res.is_err() {
      return false;
    }
    let data = data_res.unwrap();

    let padding = PaddingScheme::PKCS1v15Sign {
      hash: Some(Hash::SHA1)
    };
    let mut rng = OsRng;
    let enc_data_res = public_key.encrypt(&mut rng, padding, &data);
    if enc_data_res.is_err() {
      return false;
    }
    let enc_data = enc_data_res.unwrap();

    enc_data[..] == self.signature[..]
  }

  fn read_archive_md5_section(input: &mut R, archive_md5_section_size: u32) -> Result<Vec<ArchiveMD5SectionEntry>, PackageError> {
    let mut archive_md5_entries = Vec::<ArchiveMD5SectionEntry>::new();

    if archive_md5_section_size == 0 {
      return Ok(archive_md5_entries);
    }

    let entries = archive_md5_section_size / std::mem::size_of::<ArchiveMD5SectionEntry>() as u32;

    for _ in 0..entries {
      let mut entry = ArchiveMD5SectionEntry {
        archive_index: input.read_u32().map_err(PackageError::IOError)?,
        offset: input.read_u32().map_err(PackageError::IOError)?,
        length: input.read_u32().map_err(PackageError::IOError)?,
        checksum: Default::default()
      };

      input.read_exact(&mut entry.checksum).map_err(PackageError::IOError)?;

      archive_md5_entries.push(entry);
    }
    Ok(archive_md5_entries)
  }

  fn read_other_md5_section(input: &mut R, other_md5_section_size: u32) -> Result<([u8; 16], [u8; 16], [u8; 16]), PackageError> {
    if other_md5_section_size != 48 {
      return Err(PackageError::FileError(format!("Encountered OtherMD5Section with size of {} (should be 48)", other_md5_section_size)));
    }

    let mut tree_checksum = [0u8; 16];
    input.read_exact(&mut tree_checksum).map_err(PackageError::IOError)?;
    let mut archive_md5_entries_checksum = [0u8; 16];
    input.read_exact(&mut archive_md5_entries_checksum).map_err(PackageError::IOError)?;
    let mut whole_file_checksum = [0u8; 16];
    input.read_exact(&mut whole_file_checksum).map_err(PackageError::IOError)?;
    Ok((tree_checksum, archive_md5_entries_checksum, whole_file_checksum))
  }


  fn read_signature_section(input: &mut R, signature_section_size: u32) -> Result<(Box<[u8]>, Box<[u8]>), PackageError> {
    if signature_section_size == 0 {
      return Ok((Vec::new().into_boxed_slice(), Vec::new().into_boxed_slice()));
    }

    let public_key_size = input.read_u32().map_err(PackageError::IOError)? as usize;
    let public_key = input.read_data(public_key_size).map_err(PackageError::IOError)?;

    let signature_size = input.read_u32().map_err(PackageError::IOError)? as usize;
    let signature = input.read_data(signature_size).map_err(PackageError::IOError)?;
    Ok((public_key, signature))
  }
}
