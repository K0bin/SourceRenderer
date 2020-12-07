use sourcerenderer_bsp::zip::*;
use crate::asset::asset_manager::{AssetContainer, AssetFile, AssetFileData};
use std::io::{Cursor, Read, Seek};
use sourcerenderer_bsp::PakFile;
use std::sync::Mutex;

pub struct PakFileContainer {
  pakfile: Mutex<PakFile>
}

impl PakFileContainer {
  pub fn new(pakfile: PakFile) -> Self {
    Self {
      pakfile: Mutex::new(pakfile)
    }
  }
}

impl AssetContainer for PakFileContainer {
  fn load(&self, path: &str) -> Option<AssetFile> {
    let name = path;
    let mut guard = self.pakfile.lock().unwrap();
    let data = guard.read_entry(name)?;
    Some(AssetFile {
      path: path.to_string(),
      data: AssetFileData::Memory(Cursor::new(data))
    })
  }
}
