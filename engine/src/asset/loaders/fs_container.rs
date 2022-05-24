use std::path::{Path, PathBuf};

use sourcerenderer_core::{Platform, platform::io::IO};

use crate::asset::asset_manager::{AssetContainer, AssetFile, AssetFileData};

pub struct FSContainer {
  path: PathBuf
}

impl<P: Platform> AssetContainer<P> for FSContainer {
  fn contains(&self, path: &str) -> bool {
    let path_without_metadata = if let Some(dot_pos) = path.rfind('.') {
      if let Some(first_slash_pos) = path[dot_pos..].find('/') {
        &path[..dot_pos + first_slash_pos]
      } else {
        path
      }
    } else {
      path
    };
    self.path.join(path_without_metadata).exists()
  }
  fn load(&self, path: &str) -> Option<AssetFile<P>> {
    let path_without_metadata = if let Some(dot_pos) = path.rfind('.') {
      if let Some(first_slash_pos) = path[dot_pos..].find('/') {
        &path[..dot_pos + first_slash_pos]
      } else {
        path
      }
    } else {
      path
    };
    let file = <P::IO as IO>::open_asset(self.path.join(path_without_metadata)).ok()?;
    Some(AssetFile::<P> {
      path: path.to_string(),
      data: AssetFileData::File(file)
    })
  }
}

impl FSContainer {
  pub fn new(base_path: &str) -> Self {
    let path: PathBuf = Path::new(base_path).to_path_buf();
    Self {
      path
    }
  }
}
