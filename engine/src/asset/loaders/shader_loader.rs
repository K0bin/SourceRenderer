use std::sync::Arc;

use bevy_tasks::futures_lite::{AsyncBufReadExt, AsyncRead, AsyncReadExt};

use log::trace;
use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::Platform;

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

impl<P: Platform> AssetLoader<P> for ShaderLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path.ends_with(".json")
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        trace!("Loading shader: {:?}", &file.path);
        let mut buffer = Vec::<u8>::new();
        let res = file.read_to_end(&mut buffer).await;
        if res.is_err() {
            println!("WTF {:?}", res);
        }
        res.map_err(|_e| ())?;
        let shader: PackedShader = serde_json::from_slice(&buffer).map_err(|_e| ())?;
        manager.add_asset_data_with_progress(
            &file.path,
            AssetData::Shader(shader),
            Some(progress),
            priority,
        );
        Ok(())
    }
}
