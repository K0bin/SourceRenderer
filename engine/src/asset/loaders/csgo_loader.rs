use crate::asset::{AssetLoader, AssetType, Asset};
use sourcerenderer_core::Platform;
use std::path::Path;
use crate::asset::loaders::bsp_level::BspLevelLoader;

pub struct CSGOLoader {
  path: String
}

impl CSGOLoader {
  pub fn new(path: &str) -> Self {
    Self {
      path: path.to_owned()
    }
  }
}

impl<P: Platform> AssetLoader<P> for CSGOLoader{
  fn matches(&self, path: &str, asset_type: AssetType) -> bool {
    asset_type == AssetType::Container && Path::new(path).file_name().unwrap() == self.path.as_str()
  }

  fn load(&self, path: &str, asset_type: AssetType) -> Option<Asset<P>> {
    Some(Asset::Container(Box::new(BspLevelLoader::new(path).ok()?)))
  }
}
