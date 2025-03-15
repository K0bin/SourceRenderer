use std::io::{
    Cursor,
    Read,
};
use std::marker::PhantomData;
use std::path::PathBuf;

use regex::Regex;
use sourcerenderer_core::platform::IO;
use sourcerenderer_core::Platform;

use crate::asset::asset_manager::{
    AssetContainer,
    AssetFile,
};
use crate::asset::loaders::csgo_loader::CSGOMapLoaderError::CSGONotFound;
use crate::asset::loaders::vpk_container::{
    CSGO_PAK_NAME_PATTERN,
    CSGO_PRIMARY_PAK_NAME_PATTERN,
};

pub(super) const CSGO_MAP_NAME_PATTERN: &str = r"(de|cs|dm|am|surf|aim)_[a-zA-Z0-9_-]+\.bsp";

pub struct CSGODirectoryContainer {
    path: String,
    map_name_regex: Regex,
    primary_pak_name_regex: Regex,
    pak_name_regex: Regex,
    _p: PhantomData<<P::IO as IO>::File>,
}

unsafe impl Send for CSGODirectoryContainer {}
unsafe impl Sync for CSGODirectoryContainer {}

#[derive(Debug)]
pub enum CSGOMapLoaderError {
    CSGONotFound,
}

impl CSGODirectoryContainer {
    pub fn new(path: &str) -> Result<Self, CSGOMapLoaderError> {
        let mut exe_path = PathBuf::new();
        exe_path.push(path.to_owned());
        #[cfg(target_os = "windows")]
        exe_path.push("csgo.exe");
        #[cfg(target_os = "linux")]
        exe_path.push("csgo_linux64");

        if !<P::IO as IO>::external_asset_exists(exe_path) {
            return Err(CSGONotFound);
        }

        Ok(Self {
            path: path.to_owned(),
            map_name_regex: Regex::new(CSGO_MAP_NAME_PATTERN).unwrap(),
            primary_pak_name_regex: Regex::new(CSGO_PRIMARY_PAK_NAME_PATTERN).unwrap(),
            pak_name_regex: Regex::new(CSGO_PAK_NAME_PATTERN).unwrap(),
            _p: PhantomData,
        })
    }
}

impl AssetContainer for CSGODirectoryContainer {
    fn contains(&self, path: &str) -> bool {
        self.map_name_regex.is_match(path)
            || self.primary_pak_name_regex.is_match(path)
            || self.pak_name_regex.is_match(path)
    }

    fn load(&self, path: &str) -> Option<AssetFile> {
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

        let mut file = <P::IO as IO>::open_external_asset(&actual_path).ok()?;
        let mut buf = Vec::<u8>::new();
        file.read_to_end(&mut buf).ok()?;

        Some(AssetFile {
            path: path.to_string(),
            data: Cursor::new(buf.into_boxed_slice()),
        })
    }
}
