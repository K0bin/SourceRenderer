use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use std::thread;
use std::time::{Duration, SystemTime};

use legion::{World, Resources, Schedule};

use nalgebra::UnitQuaternion;
use sourcerenderer_core::{Platform, Vec3};

use crate::{Transform, asset::loaders::{GltfContainer, GltfLoader}, input::Input, renderer::*};
use crate::transform;
use crate::asset::{AssetManager, AssetType, AssetLoadPriority};
use crate::fps_camera;
use crate::asset::loaders::{BspLevelLoader, VPKContainerLoader, VTFTextureLoader, VMTMaterialLoader, CSGODirectoryContainer, MDLModelLoader};
use legion::query::{FilterResult, LayoutFilter};
use legion::storage::ComponentTypeId;
use crate::input::InputState;
use crate::{fps_camera::FPSCamera, renderer::RendererInterface};

pub struct TimeStampedInputState(InputState, SystemTime);

#[cfg(feature = "threading")]
pub struct Game {
  input: Arc<Input>,
  fps_camera: Mutex<FPSCamera>,
  is_running: AtomicBool
}

pub struct TickDuration(pub Duration);
pub struct TickRate(pub u32);
pub struct DeltaTime(pub Duration);
pub struct TickDelta(pub Duration);

impl DeltaTime {
  pub fn secs(&self) -> f32 {
    self.0.as_secs_f32()
  }
}

pub struct Tick(pub u64);

pub struct FilterAll {}
impl LayoutFilter for FilterAll {
  fn matches_layout(&self, _components: &[ComponentTypeId]) -> FilterResult {
    FilterResult::Match(true)
  }
}

#[cfg(feature = "threading")]
impl Game {
  pub fn run<P: Platform>(
    input: &Arc<Input>,
    renderer: &Arc<Renderer<P>>,
    asset_manager: &Arc<AssetManager<P>>,
    tick_rate: u32) -> Arc<Self> {

    asset_manager.add_loader(Box::new(BspLevelLoader::new()));
    asset_manager.add_loader(Box::new(VPKContainerLoader::new()));
    asset_manager.add_loader(Box::new(VTFTextureLoader::new()));
    asset_manager.add_loader(Box::new(VMTMaterialLoader::new()));
    asset_manager.add_loader(Box::new(MDLModelLoader::new()));

    #[cfg(target_os = "linux")]
        //let csgo_path = "~/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive";
        let csgo_path = "/run/media/robin/System/Program Files (x86)/Steam/steamapps/common/Counter-Strike Global Offensive";
    #[cfg(target_os = "windows")]
        let csgo_path = "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive";
    #[cfg(target_os = "android")]
      let csgo_path = "content://com.android.externalstorage.documents/tree/primary%3Agames%2Fcsgo/document/primary%3Agames%2Fcsgo";

    println!("Csgo path: {:?}", csgo_path);

    /*asset_manager.add_container(Box::new(GltfContainer::load("/home/robin/Projekte/bistro/bistro.glb").unwrap()));
    asset_manager.add_loader(Box::new(GltfLoader::new()));
    let mut level = asset_manager.load_level("bistro.glb/scene/Scene").unwrap();*/


    let mut level = {
      asset_manager.add_container(Box::new(CSGODirectoryContainer::new::<P>(csgo_path).unwrap()));
      let progress = asset_manager.request_asset("pak01_dir", AssetType::Container, AssetLoadPriority::Normal);
      while !progress.is_done() {
        // wait until our container is loaded
      }
      asset_manager.load_level("de_overpass.bsp").unwrap()
    };
    println!("Done loading level");

    let game = Arc::new(Self {
      input: input.clone(),
      fps_camera: Mutex::new(FPSCamera::new()),
      is_running: AtomicBool::new(true)
    });

    let c_renderer = renderer.clone();
    let c_asset_manager = asset_manager.clone();
    let c_game = game.clone();
    thread::Builder::new().name("GameThread".to_string()).spawn(move || {
      let mut world = World::default();
      let mut fixed_schedule = Schedule::builder();
      let mut schedule = Schedule::builder();
      let mut resources = Resources::default();

      crate::spinning_cube::install(&mut world, &mut resources, &mut fixed_schedule, &c_asset_manager);
      fps_camera::install::<P>(&mut world, &mut fixed_schedule);
      transform::interpolation::install(&mut fixed_schedule, &mut schedule);
      transform::install(&mut fixed_schedule);
      c_renderer.install(&mut world, &mut resources, &mut schedule);

      let point_light_entity = world.push((Transform {
        position: Vec3::new(0f32, 0f32, 0f32),
        rotation: UnitQuaternion::default(),
        scale: Vec3::new(1f32, 1f32, 1f32),
      }, PointLightComponent { intensity: 1.0f32 }));

      println!("Point Light: {:?}", point_light_entity);

      world.move_from(&mut level, &FilterAll {});

      //resources.insert(c_renderer.primary_camera().clone());

      let tick_duration = Duration::new(0, 1_000_000_000 / tick_rate);
      resources.insert(TickRate(tick_rate));
      resources.insert(TickDuration(tick_duration));

      let mut tick = 0u64;
      let mut schedule = schedule.build();
      let mut fixed_schedule = fixed_schedule.build();
      let mut last_tick_time = SystemTime::now();
      let mut last_iter_time = SystemTime::now();
      loop {
        if !c_game.is_running() {
          break;
        }
        resources.insert(c_game.input.poll());

        let now = SystemTime::now();

        // run fixed step systems first
        let mut tick_delta = now.duration_since(last_tick_time).unwrap();
        if c_renderer.is_saturated() && tick_delta <= tick_duration {
          std::thread::yield_now();
        }

        while tick_delta >= tick_duration {
          last_tick_time += tick_duration;
          resources.insert(Tick(tick));
          fixed_schedule.execute(&mut world, &mut resources);
          tick += 1;
          tick_delta = now.duration_since(last_tick_time).unwrap();
        }

        let delta = now.duration_since(last_iter_time).unwrap();
        last_iter_time = now;
        resources.insert(TickDelta(tick_delta));
        resources.insert(DeltaTime(delta));
        schedule.execute(&mut world, &mut resources);
      }
    }).unwrap();

    game
  }

  pub fn is_running(&self) -> bool {
    self.is_running.load(Ordering::SeqCst)
  }

  pub fn stop(&self) {
    self.is_running.store(false, Ordering::SeqCst);
  }
}
