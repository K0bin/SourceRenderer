use std::{marker::PhantomData, sync::Arc};

use bevy_app::{App, Plugin};
use sourcerenderer_core::Platform;
use sourcerenderer_engine::{asset::{loaders::{load_file_gltf_container, load_memory_gltf_container}, AssetLoadPriority, AssetManager, AssetType}, Engine};

use crate::{fps_camera, spinning_cube::SpinningCubePlugin};

pub struct GamePlugin<P: Platform>(PhantomData<P>);

unsafe impl<P: Platform> Send for GamePlugin<P> {}
unsafe impl<P: Platform> Sync for GamePlugin<P> {}

impl<P: Platform> Default for GamePlugin<P> {
    fn default() -> Self {
        GamePlugin(PhantomData)
    }
}

impl<P: Platform> Plugin for GamePlugin<P> {
    fn build(&self, app: &mut App) {
        {
            log::info!("Initializing GamePlugin");
            let asset_manager: &Arc<AssetManager<P>> = Engine::get_asset_manager(app);
            asset_manager.add_container_async(async move {
                log::info!("Loading GLTF file as container");
                load_file_gltf_container::<P>("bistro_sun.glb", true).await.unwrap()
            });
            asset_manager.request_asset("bistro_sun.glb/scene/Scene", AssetType::Level, AssetLoadPriority::High);
            asset_manager.request_asset("FlightHelmet/FlightHelmet.gltf/scene/0", AssetType::Level, AssetLoadPriority::High);
        }

        fps_camera::install::<P>(app);
        app.add_plugins(SpinningCubePlugin::<P>::default());
    }
}
