use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use std::time::Duration;

use legion::{Resources, Schedule, World};

use log::trace;
use nalgebra::UnitQuaternion;
use sourcerenderer_core::{Platform, Vec3, atomic_refcell::AtomicRefCell, platform::ThreadHandle};

use crate::{Transform, asset::loaders::{GltfContainer, GltfLoader}, input::Input, renderer::*};
use crate::transform;
use crate::asset::{AssetManager, AssetType, AssetLoadPriority};
use crate::fps_camera;
use crate::asset::loaders::{BspLevelLoader, VPKContainerLoader, VTFTextureLoader, VMTMaterialLoader, CSGODirectoryContainer, MDLModelLoader};
use legion::query::{FilterResult, LayoutFilter};
use legion::storage::ComponentTypeId;
use crate::input::InputState;
use crate::{fps_camera::FPSCamera, renderer::RendererInterface};
use instant::Instant;

pub struct TimeStampedInputState(InputState, Instant);

#[cfg(feature = "threading")]
pub struct Game<P: Platform> {
  input: Arc<Input>,
  fps_camera: Mutex<FPSCamera>,
  is_running: AtomicBool,
  thread_handle: AtomicRefCell<Option<P::ThreadHandle>>
}

#[derive(Debug, Clone)]
pub struct TickDuration(pub Duration);
#[derive(Debug, Clone, Copy)]
pub struct TickRate(pub u32);
#[derive(Debug, Clone)]
pub struct DeltaTime(pub Duration);
#[derive(Debug, Clone)]
pub struct TickDelta(pub Duration);

impl DeltaTime {
  pub fn secs(&self) -> f32 {
    self.0.as_secs_f32()
  }
}

#[derive(Debug, Clone, Copy)]
pub struct Tick(pub u64);

pub struct FilterAll {}
impl LayoutFilter for FilterAll {
  fn matches_layout(&self, _components: &[ComponentTypeId]) -> FilterResult {
    FilterResult::Match(true)
  }
}

#[cfg(feature = "threading")]
impl<P: Platform> Game<P> {
  pub fn run(
    platform: &P,
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
    #[cfg(target_arch = "wasm32")]
      let csgo_path = "";

    trace!("Csgo path: {:?}", csgo_path);

    let game = Arc::new(Self {
      input: input.clone(),
      fps_camera: Mutex::new(FPSCamera::new()),
      is_running: AtomicBool::new(true),
      thread_handle: AtomicRefCell::new(None)
    });

    let c_renderer = renderer.clone();
    let c_asset_manager = asset_manager.clone();
    let c_game = game.clone();
    let thread_handle = platform.start_thread("GameThread", move || {
      trace!("Started game thread");
      let mut world = World::default();
      let mut fixed_schedule = Schedule::builder();
      let mut schedule = Schedule::builder();
      let mut resources = Resources::default();



      //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("/home/robin/Projekte/SourceRenderer/MetalRoughSpheresNoTextures.glb").unwrap()));
      //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("MetalRoughSpheresNoTextures.glb").unwrap()));
      c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("/home/robin/Projekte/bistro/bistro.glb").unwrap()));
      c_asset_manager.add_loader(Box::new(GltfLoader::new()));
      let mut level = c_asset_manager.load_level("bistro.glb/scene/Scene").unwrap();
      //let mut level = c_asset_manager.load_level("MetalRoughSpheresNoTextures.glb/scene/0").unwrap();

      //let mut level = World::new(legion::WorldOptions::default());

      /*let mut level = {
        c_asset_manager.add_container(Box::new(CSGODirectoryContainer::new::<P>(csgo_path).unwrap()));
        let progress = c_asset_manager.request_asset("pak01_dir", AssetType::Container, AssetLoadPriority::Normal);
        while !progress.is_done() {
          // wait until our container is loaded
        }
        c_asset_manager.load_level("de_overpass.bsp").unwrap()
      };*/
      trace!("Done loading level");

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

      trace!("Point Light: {:?}", point_light_entity);

      world.move_from(&mut level, &FilterAll {});

      //resources.insert(c_renderer.primary_camera().clone());

      let tick_duration = Duration::new(0, 1_000_000_000 / tick_rate);
      resources.insert(TickRate(tick_rate));
      resources.insert(TickDuration(tick_duration));

      let mut tick = 0u64;
      let mut schedule = schedule.build();
      let mut fixed_schedule = fixed_schedule.build();
      let mut last_tick_time = Instant::now();
      let mut last_iter_time = Instant::now();
      loop {
        if !c_game.is_running() {
          break;
        }
        resources.insert(c_game.input.poll());

        let now = Instant::now();

        // run fixed step systems first
        let mut tick_delta = now.duration_since(last_tick_time);
        if c_renderer.is_saturated() && tick_delta <= tick_duration {
          std::thread::yield_now();
        }

        while tick_delta >= tick_duration {
          last_tick_time += tick_duration;
          resources.insert(Tick(tick));
          fixed_schedule.execute(&mut world, &mut resources);
          tick += 1;
          tick_delta = now.duration_since(last_tick_time);
        }

        let delta = now.duration_since(last_iter_time);
        last_iter_time = now;
        resources.insert(TickDelta(tick_delta));
        resources.insert(DeltaTime(delta));
        schedule.execute(&mut world, &mut resources);
      }
      c_game.is_running.store(false, Ordering::SeqCst);
    });
    {
      let mut thread_handle_guard = game.thread_handle.borrow_mut();
      *thread_handle_guard = Some(thread_handle);
    }

    game
  }

  pub fn is_running(&self) -> bool {
    self.is_running.load(Ordering::SeqCst)
  }

  pub fn stop(&self) {
    let was_running = self.is_running.swap(false, Ordering::SeqCst);
    if !was_running {
      return;
    }
    let mut thread_handle_guard = self.thread_handle.borrow_mut();
    thread_handle_guard
      .take()
      .expect("Game was already stopped")
      .join();
  }
}
