use std::{marker::PhantomData, sync::Arc};

use bevy_app::{Plugin, PreUpdate};
use bevy_ecs::schedule::Schedule;
use bevy_ecs::system::{Res, Resource};
use bevy_ecs::world::World;
use sourcerenderer_core::{Platform, PlatformPhantomData};

use crate::graphics::GPUDeviceResource;
use crate::asset::*;
use crate::asset::loaders::*;

use super::AssetManager;

#[derive(Resource)]
pub struct AssetManagerECSResource<P: Platform>(pub Arc<AssetManager<P>>);

pub struct AssetManagerPlugin<P: Platform>(PlatformPhantomData<P>);

impl<P: Platform> Default for AssetManagerPlugin<P>{ fn default() -> Self { Self(Default::default()) } }

impl<P: Platform> Plugin for AssetManagerPlugin<P> {
    fn build(&self, app: &mut bevy_app::App) {
        let gpu_device = &app.world().get_resource::<GPUDeviceResource<P::GPUBackend>>().expect("AssetManager needs GraphicsDevice atm").0;

        let asset_manager: Arc<AssetManager<P>> = AssetManager::<P>::new(gpu_device);
        asset_manager.add_container(FSContainer::new(&asset_manager));
        asset_manager.add_loader(ShaderLoader::new());

        asset_manager.add_loader(GltfLoader::new());
        asset_manager.add_loader(ImageLoader::new());
        app.insert_resource(AssetManagerECSResource(asset_manager));
        app.add_systems(PreUpdate, load_level_system::<P>);
    }
}

fn load_level_system<P: Platform>(world: &mut World) {
    let asset_manager_res = world.get_resource::<AssetManagerECSResource<P>>().unwrap();
    let asset_manager = &asset_manager_res.0;
    let level_opt = asset_manager.take_any_unintegrated_asset_data_of_type(AssetType::Level);
    if let Some(AssetData::Level(level)) = level_opt {
        level.import_into_world(world);
    }
}