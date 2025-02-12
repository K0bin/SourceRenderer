use std::{marker::PhantomData, sync::Arc};

use bevy_app::{App, FixedUpdate, Plugin, Update};
use sourcerenderer_core::Platform;
use sourcerenderer_engine::{asset::{loaders::GltfContainer, AssetLoadPriority, AssetManager, AssetType}, Engine};

use crate::{fps_camera::{fps_camera_movement, retrieve_fps_camera_rotation}, spinning_cube::SpinningCubePlugin};

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
                GltfContainer::<P>::load("bistro_sun.glb", true).await.unwrap()
            });
            asset_manager.request_asset("bistro_sun.glb/scene/Scene", AssetType::Level, AssetLoadPriority::High);
        }

        app
            .add_systems(FixedUpdate, fps_camera_movement::<P>)
            .add_systems(Update, retrieve_fps_camera_rotation::<P>)
            .add_plugins(SpinningCubePlugin::<P>::default());
    }
}
