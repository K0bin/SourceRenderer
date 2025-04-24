use std::marker::PhantomData;
use std::sync::Arc;

use bevy_app::{
    Plugin,
    PreUpdate,
};
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use sourcerenderer_core::platform::PlatformIO;

use super::AssetManager;
use crate::asset::loaders::*;
use crate::asset::*;

#[derive(Resource)]
pub struct AssetManagerECSResource(pub Arc<AssetManager>);

pub struct AssetManagerPlugin<IO: PlatformIO>(PhantomData<IO>);
unsafe impl<IO: PlatformIO> Send for AssetManagerPlugin<IO> {}
unsafe impl<IO: PlatformIO> Sync for AssetManagerPlugin<IO> {}

impl<IO: PlatformIO> Default for AssetManagerPlugin<IO> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<IO: PlatformIO> Plugin for AssetManagerPlugin<IO> {
    fn build(&self, app: &mut bevy_app::App) {
        let asset_manager: Arc<AssetManager> = AssetManager::new();
        asset_manager.add_container(FSContainer::<IO>::new(&asset_manager));
        asset_manager.add_loader(ShaderLoader::new());

        asset_manager.add_loader(GltfLoader::new());
        asset_manager.add_loader(ImageLoader::new());
        app.insert_resource(AssetManagerECSResource(asset_manager));
        app.add_systems(PreUpdate, load_level_system);
    }
}

fn load_level_system(world: &mut World) {
    let asset_manager_res = world.get_resource::<AssetManagerECSResource>().unwrap();
    let asset_manager = &asset_manager_res.0;
    let level_opt = asset_manager.receive_asset_data(AssetTypeGroup::Level);
    if let Some(LoadedAssetData {
        data: AssetData::Level(level),
        ..
    }) = level_opt
    {
        level.import_into_world(world);
    }
}
