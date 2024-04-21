use std::io::Read;
use std::sync::Arc;

use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::Platform;

use crate::asset::asset_manager::{
    AssetFile,
    AssetLoaderResult,
};
use crate::asset::{
    Asset,
    AssetLoadPriority,
    AssetLoader,
    AssetLoaderProgress,
    AssetManager,
};

pub struct ShaderLoader {}

impl ShaderLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl<P: Platform> AssetLoader<P> for ShaderLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        if cfg!(target_arch = "wasm32") {
            file.path.ends_with(".glsl")
        } else {
            file.path.ends_with(".json")
        }
    }

    fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<AssetLoaderResult, ()> {
        let mut buffer = Vec::<u8>::new();
        file.data.read_to_end(&mut buffer).map_err(|_e| ())?;
        let shader: PackedShader = serde_json::from_slice(&buffer).map_err(|_e| ())?;
        manager.add_asset_with_progress(
            &file.path,
            Asset::Shader(shader),
            Some(progress),
            priority,
        );
        Ok(AssetLoaderResult::None)
    }
}
