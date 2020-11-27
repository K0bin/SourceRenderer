use crate::asset::{AssetLoader, AssetType, Asset};
use sourcerenderer_core::Platform;
use std::path::{Path, PathBuf};
use crate::asset::asset_manager::{AssetLoaderResult, AssetContainer, AssetFile, AssetFileData};
use crate::asset::loaders::csgo_loader::CSGOMapLoaderError::CSGONotFound;
use regex::Regex;
use std::fs::File;

pub(super) const CSGO_MAP_NAME_PATTERN: &str = r"(de|cs|dm|am|surf|aim)_[a-zA-Z0-9_-]+";

pub struct CSGODirectoryContainer {
  path: String,
  map_name_regex: Regex
}

#[derive(Debug)]
pub enum CSGOMapLoaderError {
  CSGONotFound
}

impl CSGODirectoryContainer {
  pub fn new(path: &str) -> Result<Self, CSGOMapLoaderError> {
    let mut exe_path = PathBuf::new();
    exe_path.push(path.to_owned());
    exe_path.push("csgo.exe");

    if !exe_path.exists() {
      return Err(CSGONotFound);
    }

    Ok(Self {
      path: path.to_owned(),
      map_name_regex: Regex::new(CSGO_MAP_NAME_PATTERN).unwrap()
    })
  }
}

impl AssetContainer for CSGODirectoryContainer {
  fn load(&self, path: &str) -> Option<AssetFile> {
    let actual_path = if self.map_name_regex.is_match(path) {
      let mut actual_path = PathBuf::new();
      actual_path.push(&self.path);
      actual_path.push("csgo");
      actual_path.push("maps");
      let mut file_name = path.to_owned();
      file_name.push_str(".bsp");
      actual_path.push(file_name);
      actual_path
    } else {
      let mut actual_path = PathBuf::new();
      actual_path.push(path);
      actual_path
    };

    let file = File::open(&actual_path);
    file.ok().map(|file|
      AssetFile {
      path: path.to_owned(),
      data: AssetFileData::File(file)
    })
  }
}
