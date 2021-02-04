use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use legion::{World, Resources, Schedule};

use sourcerenderer_core::Platform;

use crate::renderer::*;
use crate::transform;
use crate::asset::{AssetManager, AssetType, AssetLoadPriority};
use crate::fps_camera;
use crate::asset::loaders::{CSGODirectoryContainer, BspLevelLoader, VPKContainerLoader, VTFTextureLoader, VMTMaterialLoader};
use legion::query::{FilterResult, LayoutFilter};
use legion::storage::ComponentTypeId;

pub struct Scene {}

pub struct TickDuration(pub Duration);
pub struct TickRate(pub u32);
pub struct DeltaTime(pub Duration);
pub struct TickDelta(pub Duration);

impl DeltaTime {
  pub fn secs(&self) -> f32 {
    self.0.as_secs_f32()
  }
}

pub struct Tick(u64);

pub struct FilterAll {}
impl LayoutFilter for FilterAll {
  fn matches_layout(&self, components: &[ComponentTypeId]) -> FilterResult {
    FilterResult::Match(true)
  }
}

impl Scene {
  pub fn run<P: Platform>(renderer: &Arc<Renderer<P>>,
                          asset_manager: &Arc<AssetManager<P>>,
                          input: &Arc<P::Input>,
                          tick_rate: u32) {
    asset_manager.add_loader(Box::new(BspLevelLoader::new()));
    asset_manager.add_loader(Box::new(VPKContainerLoader::new()));
    asset_manager.add_loader(Box::new(VTFTextureLoader::new()));
    asset_manager.add_loader(Box::new(VMTMaterialLoader::new()));
    #[cfg(target_os = "linux")]
        //let csgo_path = "~/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive";
        let csgo_path = "/run/media/robin/System/Program Files (x86)/Steam/steamapps/common/Counter-Strike Global Offensive";
    #[cfg(target_os = "windows")]
        let csgo_path = "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive";
    asset_manager.add_container(Box::new(CSGODirectoryContainer::new(csgo_path).unwrap()));
    let progress = asset_manager.request_asset("pak01_dir", AssetType::Container, AssetLoadPriority::Normal);
    while !progress.is_done() {
      // wait until our container is loaded
    }
    let mut level = asset_manager.load_level("de_overpass.bsp").unwrap();

    let c_renderer = renderer.clone();
    let c_asset_manager = asset_manager.clone();
    let c_input = input.clone();
    thread::Builder::new().name("GameThread".to_string()).spawn(move || {
      let mut world = World::default();
      let mut fixed_schedule = Schedule::builder();
      let mut schedule = Schedule::builder();
      let mut resources = Resources::default();

      resources.insert(c_input);

      crate::spinning_cube::install(&mut world, &mut resources, &mut fixed_schedule, &c_asset_manager);
      fps_camera::install::<P>(&mut world, &mut fixed_schedule);
      transform::interpolation::install(&mut fixed_schedule, &mut schedule);
      transform::install(&mut fixed_schedule);
      c_renderer.install(&mut world, &mut resources, &mut schedule);

      world.move_from(&mut level, &FilterAll {});

      resources.insert(c_renderer.primary_camera().clone());

      let tick_duration = Duration::new(0, (1_000_000_000 / tick_rate));
      resources.insert(TickRate(tick_rate));
      resources.insert(TickDuration(tick_duration));

      let mut tick = 0u64;
      let mut schedule = schedule.build();
      let mut fixed_schedule = fixed_schedule.build();
      let mut last_tick_time = SystemTime::now();
      let mut last_iter_time = SystemTime::now();
      loop {
        while c_renderer.is_saturated() {}

        let now = SystemTime::now();

        // run fixed step systems first
        let mut tick_delta = now.duration_since(last_tick_time).unwrap();
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
    });
  }
}
