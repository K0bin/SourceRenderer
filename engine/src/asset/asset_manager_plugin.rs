use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use bevy_app::{Plugin, PreUpdate};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::{Commands, Res};
use bevy_ecs::query::Added;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};
use bevy_transform::components::Transform;
use sourcerenderer_core::platform::PlatformIO;

use super::AssetManager;
use crate::asset::loaded_level::LevelData;
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
        //asset_manager.add_loader(RawVolumeLoader::new());
        asset_manager.add_loader(RawVolumeLoaderTexture::new());
        app.insert_resource(AssetManagerECSResource(asset_manager));
        app.insert_resource(LevelTemplates(HashMap::new()));
        app.add_systems(PreUpdate, receive_level_system);
        app.add_systems(PreUpdate, load_level_system);
    }
}

#[derive(Component)]
pub struct PlacedLevel(pub LevelHandle);

#[derive(Resource)]
pub struct LevelTemplates(HashMap<LevelHandle, LevelData>);

fn receive_level_system(
    mut commands: Commands,
    asset_manager_res: Res<AssetManagerECSResource>,
    entities: Query<(Entity, &PlacedLevel, Option<&Transform>)>,
    mut templates: ResMut<LevelTemplates>) {

    for (entity, _, transform_opt) in entities {
        if transform_opt.is_none() {
            commands.entity(entity).insert(Transform::default());
        }
    }

    let asset_manager = &asset_manager_res.0;
    let mut level_opt: Option<LoadedAssetData> = asset_manager.receive_asset_data(AssetTypeGroup::Level);
    while let Some(LoadedAssetData {
                       data: AssetData::Level(level),
                       handle,
                       ..
                   }) = level_opt {

        for (entity, placed_level, transform_opt) in entities {
            if placed_level.0 == handle.into() {
               level.import_into_world(entity, &mut commands);
            }
            if transform_opt.is_none() {
                commands.entity(entity).insert(Transform::default());
            }
        }

        templates.0.insert(handle.into(), level);

        level_opt = asset_manager.receive_asset_data(AssetTypeGroup::Level);
    }
}

fn load_level_system(mut commands: Commands, entities: Query<(Entity, &PlacedLevel, Option<&Transform>), Added<PlacedLevel>>, templates: Res<LevelTemplates>) {
    for (entity, placed_level, transform_opt) in entities {
        if transform_opt.is_none() {
            commands.entity(entity).insert(Transform::default());

            let level_data = templates.0.get(&placed_level.0);
            if let Some(level) = level_data {
                level.import_into_world(entity, &mut commands);
            }
        }
    }
}
