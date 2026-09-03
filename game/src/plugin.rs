use std::{marker::PhantomData, sync::Arc};

use crate::{RendererPicker, fps_camera, spinning_cube::SpinningCubePlugin};
use bevy_app::{App, Plugin};
use sourcerenderer_core::platform::PlatformIO;
use sourcerenderer_engine::renderer::RendererType;
use sourcerenderer_engine::{
    Engine,
    asset::{AssetLoadPriority, AssetManager, AssetType, loaders::load_file_gltf_container},
};

pub struct GamePlugin<IO: PlatformIO>(PhantomData<IO>);

unsafe impl<IO: PlatformIO> Send for GamePlugin<IO> {}
unsafe impl<IO: PlatformIO> Sync for GamePlugin<IO> {}

impl<IO: PlatformIO> Default for GamePlugin<IO> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<IO: PlatformIO> Plugin for GamePlugin<IO> {
    fn build(&self, app: &mut App) {
        {
            log::info!("Initializing GamePlugin");
            let asset_manager: &Arc<AssetManager> = Engine::get_asset_manager(app);
            /*asset_manager.add_container_async(async move {
                log::info!("Loading GLTF file as container");
                load_file_gltf_container::<IO>("bistro_sun.glb", true).await.unwrap()
            });
            asset_manager.request_asset("bistro_sun.glb/scene/Scene", AssetType::Level, AssetLoadPriority::High);*/
            asset_manager.request_asset(
                "FlightHelmet/FlightHelmet.gltf/scene/0",
                AssetType::Level,
                AssetLoadPriority::High,
            );
        }

        fps_camera::install(app);
        app.add_plugins(SpinningCubePlugin);
    }
}

impl<IO: PlatformIO> RendererPicker for GamePlugin<IO> {
    fn pick_renderer() -> RendererType {
        RendererType::Regular
    }
}
