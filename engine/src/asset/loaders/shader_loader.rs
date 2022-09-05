use std::{sync::Arc, io::Read};

use sourcerenderer_core::Platform;

use crate::asset::{AssetLoader, asset_manager::{AssetFile, AssetLoaderResult}, AssetManager, AssetLoadPriority, AssetLoaderProgress, Asset};

pub struct ShaderLoader {}

impl ShaderLoader {
  pub fn new() -> Self {
    Self {}
  }
}

impl<P: Platform> AssetLoader<P> for ShaderLoader {
  fn matches(&self, file: &mut AssetFile) -> bool {
    file.path.ends_with(".spv")
  }

  fn load(&self, mut file: AssetFile, manager: &Arc<AssetManager<P>>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let mut buffer = Vec::<u8>::new();
    file.data.read_to_end(&mut buffer).map_err(|_e| ())?;
    manager.add_asset_with_progress(&file.path, Asset::Shader(buffer.into_boxed_slice()), Some(progress), priority);
    Ok(AssetLoaderResult {
      level: None
    })
  }
}
