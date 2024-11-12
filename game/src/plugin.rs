use std::marker::PhantomData;

use bevy_app::{App, FixedUpdate, Plugin, Update};
use sourcerenderer_core::Platform;
use sourcerenderer_engine::{asset::{loaders::GltfContainer, AssetManager}, Engine};

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
            let asset_manager: &AssetManager<P> = Engine::get_asset_manager(app);

            asset_manager.add_container(Box::new(
                GltfContainer::<P>::load("bistro_sun.glb", true)
                    .unwrap(),
            ));

            let level = asset_manager.load_level("bistro_sun.glb/scene/Scene").unwrap();
            let world = app.world_mut();
            level.import_into_world(world);
        }

        //app.world_mut().entities_mut()

        app
            .add_systems(FixedUpdate, fps_camera_movement::<P>)
            .add_systems(Update, retrieve_fps_camera_rotation::<P>)
            .add_plugins(SpinningCubePlugin::<P>::default());
    }
}
