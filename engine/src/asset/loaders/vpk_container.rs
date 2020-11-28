use std::io::BufReader;
use std::fs::File;

use sourcerenderer_vpk::{Package, PackageError};
use crate::asset::{AssetLoader, Asset};
use crate::asset::asset_manager::{AssetLoaderContext, AssetLoaderResult, AssetFile, AssetFileData, LoadedAsset, AssetContainer};
use sourcerenderer_core::Platform;
use regex::Regex;
use std::path::Path;

pub(super) const CSGO_PAK_NAME_PATTERN: &str = r"pak01_dir(\.bsp)*";

pub struct VPKContainer {
  package: Package<BufReader<File>>
}

impl VPKContainer {
 pub fn new(asset_file: AssetFile) -> Result<Self, PackageError> {
   let path = asset_file.path.clone();
   let file = match asset_file.data {
     AssetFileData::File(file) => file,
     _ => unreachable!()
   };
   let reader = BufReader::new(file);
   Ok(Self {
     package: Package::read(&path, reader)?
   })
 }
}

impl AssetContainer for VPKContainer {
  fn load(&self, path: &str) -> Option<AssetFile> {
    let entry = self.package.find_entry(path);
    entry
      .and_then(|entry| self.package.read_entry(entry, false).ok())
      .map(|data| AssetFile {
        path: path.to_string(),
        data: AssetFileData::Memory(data)
      })
  }
}

pub struct VPKContainerLoader {
  pak_name_regex: Regex
}

impl VPKContainerLoader {
  pub fn new() -> Self {
    Self {
      pak_name_regex: Regex::new(CSGO_PAK_NAME_PATTERN).unwrap()
    }
  }
}

impl<P: Platform> AssetLoader<P> for VPKContainerLoader {
  fn matches(&self, file: &AssetFile) -> bool {
    let file_name = Path::new(&file.path).file_stem();
    file_name.and_then(|file_name| file_name.to_str()).map_or(false, |file_name| self.pak_name_regex.is_match(file_name))
  }

  fn load(&self, file: AssetFile, _context: &AssetLoaderContext<P>) -> Result<AssetLoaderResult<P>, ()> {
    let path = file.path.clone();
    let container = Box::new(VPKContainer::new(file).unwrap());
    Ok(AssetLoaderResult {
      assets: vec![
        LoadedAsset {
          path,
          asset: Asset::Container(container)
        }
      ],
      requests: vec![]
    })
  }
}
