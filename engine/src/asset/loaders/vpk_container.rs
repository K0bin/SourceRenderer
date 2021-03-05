use std::io::{Cursor, Error as IOError, ErrorKind};

use sourcerenderer_vpk::{Package, PackageError};
use crate::asset::{AssetLoader, AssetManager, AssetLoaderProgress};
use crate::asset::asset_manager::{AssetLoaderResult, AssetFile, AssetFileData, AssetContainer, AssetLoadPriority};
use sourcerenderer_core::Platform;
use regex::Regex;
use std::path::Path;
use std::sync::Arc;

pub(super) const CSGO_PRIMARY_PAK_NAME_PATTERN: &str = r"pak01_dir(\.vpk)*";
pub(super) const CSGO_PAK_NAME_PATTERN: &str = r"pak[0-9]*[0-9]_[0-9]+(\.vpk)*";

pub struct VPKContainer<P: Platform> {
  package: Package<AssetFile<P>>
}

pub fn new_vpk_container<P: Platform>(asset_manager: &Arc<AssetManager<P>>, asset_file: AssetFile<P>) -> Result<Box<dyn AssetContainer<P>>, PackageError> {
  let path = asset_file.path.clone();
  let asset_manager = Arc::downgrade(asset_manager);

  Package::read(&path, asset_file, move |path| {
    let mgr = asset_manager.upgrade()
      .ok_or(IOError::new(ErrorKind::Other, "Manager dropped"))?;
    mgr.load_file(path)
      .ok_or(IOError::new(ErrorKind::NotFound, "File not found"))
  }).map(|package|
    Box::new(VPKContainer::<P> {
      package
  }) as Box<dyn AssetContainer<P>>)
}

impl<P: Platform> AssetContainer<P> for VPKContainer<P> {
  fn contains(&self, path: &str) -> bool {
    self.package.find_entry(path).is_some()
  }

  fn load(&self, path: &str) -> Option<AssetFile<P>> {
    let entry = self.package.find_entry(path);
    entry
      .and_then(|entry| self.package.read_entry(entry, false).ok())
      .map(|data| AssetFile {
        path: path.to_string(),
        data: AssetFileData::Memory(Cursor::new(data))
      })
  }
}

pub struct VPKContainerLoader {
  pak_name_regex: Regex
}

impl VPKContainerLoader {
  pub fn new() -> Self {
    Self {
      pak_name_regex: Regex::new(CSGO_PRIMARY_PAK_NAME_PATTERN).unwrap()
    }
  }
}

impl<P: Platform> AssetLoader<P> for VPKContainerLoader {
  fn matches(&self, file: &mut AssetFile<P>) -> bool {
    let file_name = Path::new(&file.path).file_stem();
    file_name.and_then(|file_name| file_name.to_str()).map_or(false, |file_name| self.pak_name_regex.is_match(file_name))
  }

  fn load(&self, file: AssetFile<P>, manager: &Arc<AssetManager<P>>, _priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let container = new_vpk_container::<P>(manager, file).unwrap();
    manager.add_container_with_progress(container, Some(progress));
    Ok(AssetLoaderResult {
      level: None
    })
  }
}
