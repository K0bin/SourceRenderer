use std::sync::Arc;

use io_util::ReadEntireSeekableFileAsync as _;
use log::trace;
use crate::graphics::gpu::PackedShader;

use crate::asset::asset_manager::AssetFile;
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager
};

pub struct ShaderLoader {}

impl ShaderLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl AssetLoader for ShaderLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path().ends_with(".json")
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        trace!("Loading shader: {:?}", file.path());
        let file_res = file.read_seekable_to_end().await;
        if let Err(e) = &file_res {
            log::error!("Loading shader file failed: {:?}", e);
            return Err(());
        }
        let buffer = file_res.unwrap();
        let shader_res = serde_json::from_slice(&buffer);
        if let Err(e) = &shader_res {
            log::error!("Deserializing shader file failed: {:?}", e);
            return Err(());
        }
        let shader: PackedShader = shader_res.unwrap();
        manager.add_asset_data_with_progress(
            &file.path(),
            AssetData::Shader(shader),
            Some(progress),
            priority,
        );
        Ok(())
    }
}
