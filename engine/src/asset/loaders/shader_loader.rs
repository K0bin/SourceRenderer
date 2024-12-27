use std::sync::Arc;

use bevy_tasks::futures_lite::{AsyncBufReadExt, AsyncRead, AsyncReadExt};

use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::Platform;

use crate::asset::asset_manager::{
    AssetFile,
    DirectlyLoadedAsset
};
use crate::asset::{
    Asset, AssetLoadPriority, AssetLoader, AssetLoaderAsync, AssetLoaderProgress, AssetManager
};

pub struct ShaderLoader {}

impl ShaderLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl<P: Platform> AssetLoaderAsync<P> for ShaderLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        if cfg!(target_arch = "wasm32") {
            file.path.ends_with(".glsl")
        } else {
            file.path.ends_with(".json")
        }
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        println!("Loading shader: {:?}", &file.path);
        let mut buffer = Vec::<u8>::new();
        let res = file.read_to_end(&mut buffer).await;
        if res.is_err() {
            println!("WTF {:?}", res);
        }
        res.map_err(|_e| ())?;
        let shader: PackedShader = serde_json::from_slice(&buffer).map_err(|_e| ())?;
        manager.add_asset_with_progress(
            &file.path,
            Asset::Shader(shader),
            Some(progress),
            priority,
        );
        Ok(DirectlyLoadedAsset::None)
    }
}
