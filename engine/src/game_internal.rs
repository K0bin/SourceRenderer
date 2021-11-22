use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use std::time::Duration;

use legion::{Resources, Schedule, World};

use log::trace;
use nalgebra::UnitQuaternion;
use sourcerenderer_core::{Platform, Vec3, atomic_refcell::AtomicRefCell, platform::ThreadHandle};

use crate::{DeltaTime, Tick, TickDelta, TickDuration, TickRate, Transform, asset::loaders::{GltfContainer, GltfLoader}, game::FilterAll, input::Input, renderer::*};
use crate::transform;
use crate::asset::{AssetManager, AssetType, AssetLoadPriority};
use crate::fps_camera;
use crate::asset::loaders::{BspLevelLoader, VPKContainerLoader, VTFTextureLoader, VMTMaterialLoader, CSGODirectoryContainer, MDLModelLoader};
use legion::query::{FilterResult, LayoutFilter};
use legion::storage::ComponentTypeId;
use crate::input::InputState;
use crate::{fps_camera::FPSCamera, renderer::RendererInterface};
use instant::Instant;
use crate::game::Game;

pub struct GameInternal {
  world: World,
  last_tick_time: Instant,
  last_iter_time: Instant,
  schedule: Schedule,
  fixed_schedule: Schedule,
  resources: Resources,
  tick: u64,
  tick_duration: Duration
}

impl GameInternal {
  pub fn new<P: Platform>(asset_manager: &Arc<AssetManager<P>>, renderer: &Arc<Renderer<P>>, tick_rate: u32) -> Self {
    let mut world = World::default();
    let mut fixed_schedule = Schedule::builder();
    let mut schedule = Schedule::builder();
    let mut resources = Resources::default();

    //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("/home/robin/Projekte/SourceRenderer/MetalRoughSpheresNoTextures.glb").unwrap()));
    //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("MetalRoughSpheresNoTextures.glb").unwrap()));
    //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("/home/robin/Projekte/bistro/bistro.glb").unwrap()));
    //c_asset_manager.add_loader(Box::new(GltfLoader::new()));
    //let mut level = c_asset_manager.load_level("bistro.glb/scene/Scene").unwrap();
    //let mut level = c_asset_manager.load_level("MetalRoughSpheresNoTextures.glb/scene/0").unwrap();

    let mut level = World::new(legion::WorldOptions::default());

    /*let mut level = {
      c_asset_manager.add_container(Box::new(CSGODirectoryContainer::new::<P>(csgo_path).unwrap()));
      let progress = c_asset_manager.request_asset("pak01_dir", AssetType::Container, AssetLoadPriority::Normal);
      while !progress.is_done() {
        // wait until our container is loaded
      }
      c_asset_manager.load_level("de_overpass.bsp").unwrap()
    };*/
    trace!("Done loading level");

    crate::spinning_cube::install(&mut world, &mut resources, &mut fixed_schedule, &asset_manager);
    fps_camera::install::<P>(&mut world, &mut fixed_schedule);
    transform::interpolation::install(&mut fixed_schedule, &mut schedule);
    transform::install(&mut fixed_schedule);
    renderer.install(&mut world, &mut resources, &mut schedule);

    let point_light_entity = world.push((Transform {
      position: Vec3::new(0f32, 0f32, 0f32),
      rotation: UnitQuaternion::default(),
      scale: Vec3::new(1f32, 1f32, 1f32),
    }, PointLightComponent { intensity: 1.0f32 }));

    trace!("Point Light: {:?}", point_light_entity);

    world.move_from(&mut level, &FilterAll {});

    //resources.insert(c_renderer.primary_camera().clone());

    let tick_duration = Duration::new(0, 1_000_000_000 / tick_rate);
    resources.insert(TickRate(tick_rate));
    resources.insert(TickDuration(tick_duration));

    let schedule = schedule.build();
    let fixed_schedule = fixed_schedule.build();
    let last_tick_time = Instant::now();
    let last_iter_time = Instant::now();

    Self {
      last_iter_time,
      last_tick_time,
      world,
      fixed_schedule,
      schedule,
      resources,
      tick: 0,
      tick_duration
    }
  }

  pub fn update<P: Platform>(&mut self, game: &Game<P>, renderer: &Arc<Renderer<P>>) -> bool {
    self.resources.insert(game.input().poll());

    let now = Instant::now();

    // run fixed step systems first
    let mut tick_delta = now.duration_since(self.last_tick_time);
    if renderer.is_saturated() && tick_delta <= self.tick_duration {
      std::thread::yield_now();
      return false;
    }

    while tick_delta >= self.tick_duration {
      self.last_tick_time += self.tick_duration;
      self.resources.insert(Tick(self.tick));
      self.fixed_schedule.execute(&mut self.world, &mut self.resources);
      self.tick += 1;
      tick_delta = now.duration_since(self.last_tick_time);
    }

    let delta = now.duration_since(self.last_iter_time);
    self.last_iter_time = now;
    self.resources.insert(TickDelta(tick_delta));
    self.resources.insert(DeltaTime(delta));
    self.schedule.execute(&mut self.world, &mut self.resources);
    return true;
  }
}
