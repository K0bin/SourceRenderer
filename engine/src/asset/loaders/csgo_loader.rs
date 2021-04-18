use std::path::PathBuf;
use crate::asset::asset_manager::{AssetContainer, AssetFile, AssetFileData};
use crate::asset::loaders::csgo_loader::CSGOMapLoaderError::CSGONotFound;
use regex::Regex;
use crate::asset::loaders::vpk_container::{CSGO_PRIMARY_PAK_NAME_PATTERN, CSGO_PAK_NAME_PATTERN};
use sourcerenderer_core::Platform;
use sourcerenderer_core::platform::io::IO;

pub(super) const CSGO_MAP_NAME_PATTERN: &str = r"(de|cs|dm|am|surf|aim)_[a-zA-Z0-9_-]+\.bsp";

pub struct CSGODirectoryContainer {
  path: String,
  map_name_regex: Regex,
  primary_pak_name_regex: Regex,
  pak_name_regex: Regex
}

#[derive(Debug)]
pub enum CSGOMapLoaderError {
  CSGONotFound
}

impl CSGODirectoryContainer {
  pub fn new<P: Platform>(path: &str) -> Result<Self, CSGOMapLoaderError> {
    let mut exe_path = PathBuf::new();
    exe_path.push(path.to_owned());
    exe_path.push("csgo.exe");

    if !<P::IO as IO>::external_asset_exists(exe_path) {
      return Err(CSGONotFound);
    }

    Ok(Self {
      path: path.to_owned(),
      map_name_regex: Regex::new(CSGO_MAP_NAME_PATTERN).unwrap(),
      primary_pak_name_regex: Regex::new(CSGO_PRIMARY_PAK_NAME_PATTERN).unwrap(),
      pak_name_regex: Regex::new(CSGO_PAK_NAME_PATTERN).unwrap()
    })
  }
}

impl<P: Platform> AssetContainer<P> for CSGODirectoryContainer {
  fn contains(&self, path: &str) -> bool {
    return self.map_name_regex.is_match(path) || self.primary_pak_name_regex.is_match(path) || self.pak_name_regex.is_match(path);
  }

  fn load(&self, path: &str) -> Option<AssetFile<P>> {
    let actual_path = if self.map_name_regex.is_match(path) {
      let mut actual_path = PathBuf::new();
      actual_path.push(&self.path);
      actual_path.push("csgo");
      actual_path.push("maps");
      let mut file_name = path.to_owned();
      if !file_name.ends_with(".bsp") {
        file_name.push_str(".bsp");
      }
      actual_path.push(file_name);
      actual_path
    } else if self.primary_pak_name_regex.is_match(path) || self.pak_name_regex.is_match(path) {
      let mut actual_path = PathBuf::new();
      actual_path.push(&self.path);
      actual_path.push("csgo");
      let mut file_name = path.to_owned();
      if !file_name.ends_with(".vpk") {
        file_name.push_str(".vpk");
      }
      actual_path.push(file_name);
      actual_path
    } else {
      return None;
    };

    let file = <P::IO as IO>::open_external_asset(&actual_path);
    file.ok().map(|file|
      AssetFile {
      path: path.to_string(),
      data: AssetFileData::File(file)
    })
  }
}
