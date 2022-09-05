use std::{io::{Cursor, Error as IOError, ErrorKind, Result as IOResult, Read, Seek, BufReader, SeekFrom}, path::Path, usize, sync::Mutex};
use log::warn;
use sourcerenderer_core::{Platform, platform::IO};

use crate::asset::{asset_manager::{AssetContainer, AssetFile}, loaders::gltf::glb};

pub struct GltfContainer<P: Platform> {
  json_offset: u64,
  data_offset: u64,
  reader: Mutex<BufReader<<P::IO as IO>::File>>,
  base_path: String,
  scene_base_path: String,
  buffer_base_path: String,
  texture_base_path: String,
}

impl<P: Platform> GltfContainer<P> {
  pub fn load(path: &str) -> IOResult<Self> {

    let mut file = BufReader::new(P::IO::open_external_asset(path)?);
    let header = glb::GlbHeader::read(&mut file)?;

    let json_chunk_header = glb::GlbChunkHeader::read(&mut file)?;
    let json_offset = file.seek(SeekFrom::Current(0))?;
    file.seek_relative(json_chunk_header.length as i64)?;

    let data_chunk_header = glb::GlbChunkHeader::read(&mut file)?;
    let data_offset = file.seek(SeekFrom::Current(0))?;

    if data_offset + data_chunk_header.length as u64 != header.length as u64 {
      warn!("GLB file contains more than 3 chunks. This is currently unsupported.");
      return Err(IOError::new(ErrorKind::Other, "GLB file contains more than 3 chunks. This is currently unsupported."));
    }

    let file_name = Path::new(path).file_name().expect("Failed to read file name");
    let base_path = file_name.to_str().unwrap().to_string() + "/";

    let scene_base_path = base_path.clone() + "scene/";
    let buffer_base_path = base_path.clone() + "buffer/";
    let texture_base_path = base_path.clone() + "texture/";

    Ok(Self {
      reader: Mutex::new(file),
      json_offset,
      data_offset,
      base_path,
      scene_base_path,
      texture_base_path,
      buffer_base_path
    })
  }
}

impl<P: Platform> AssetContainer for GltfContainer<P> {
  fn load(&self, path: &str) -> Option<crate::asset::asset_manager::AssetFile> {
    let mut reader = self.reader.lock().unwrap();
    if path.starts_with(&self.scene_base_path) {
      let length = (self.data_offset - self.json_offset - glb::GlbChunkHeader::size()) as usize;
      let mut buffer = Vec::with_capacity(length);
      unsafe {
        buffer.set_len(length);
      }
      reader.seek(SeekFrom::Start(self.json_offset)).ok()?;
      reader.read_exact(&mut buffer).ok()?;
      return Some(AssetFile {
        path: path.to_string(),
        data: Cursor::new(buffer.into_boxed_slice())
      });
    }
    let is_texture = path.starts_with(&self.texture_base_path);
    let is_buffer = path.starts_with(&self.buffer_base_path);
    if is_buffer || is_texture {
      let base_path = if is_buffer { &self.buffer_base_path } else { &self.texture_base_path };
      let parts: Vec<&str> = path[base_path.len()..].split('-').collect();
      let offset: u64 = parts[0].parse().unwrap();
      let mut end = parts[1];
      if let Some(pos) = end.find('.') {
        end = &end[..pos];
      }
      let length: u64 = end.parse().unwrap();

      let mut buffer = Vec::with_capacity(length as usize);
      unsafe {
        buffer.set_len(length as usize);
      }
      reader.seek(SeekFrom::Start(self.data_offset + offset)).ok()?;
      reader.read_exact(&mut buffer).ok()?;

      return Some(AssetFile {
        path: path.to_string(),
        data: Cursor::new(buffer.into_boxed_slice())
      });
    }

    None
  }
}
