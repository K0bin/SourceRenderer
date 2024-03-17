use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, TryRecvError};
use instant::Instant;
use legion::{
    Resources,
    Schedule,
    World,
};
use log::trace;
use nalgebra::UnitQuaternion;
use sourcerenderer_core::platform::Event;
use sourcerenderer_core::{
    Platform,
    Vec3, Vec2UI,
};

use crate::asset::loaders::{
    GltfContainer,
    GltfLoader,
    ImageLoader,
};
use crate::asset::AssetManager;
use crate::game::{
    FilterAll,
    Game,
};
use crate::physics::PhysicsWorld;
use crate::renderer::{
    RendererInterface,
    *,
};
use crate::ui::UI;
use crate::{
    fps_camera,
    transform,
    DeltaTime,
    Tick,
    TickDelta,
    TickDuration,
    TickRate,
    Transform,
};

pub struct GameInternal<P: Platform> {
    world: World,
    last_tick_time: Instant,
    last_iter_time: Instant,
    schedule: Schedule,
    fixed_schedule: Schedule,
    resources: Resources,
    tick: u64,
    tick_duration: Duration,
    window_event_receiver: Receiver<Event<P>>,
    ui: UI<P>
}

impl<P: Platform> GameInternal<P> {
    pub fn new(
        asset_manager: &Arc<AssetManager<P>>,
        renderer: &Arc<Renderer<P>>,
        tick_rate: u32,
        window_event_receiver: Receiver<Event<P>>,
        window_size: Vec2UI
    ) -> Self {
        let ui = UI::new(renderer.device(), window_size);

        let mut world = World::default();
        let mut fixed_schedule = Schedule::builder();
        let mut schedule = Schedule::builder();
        let mut resources = Resources::default();
        let tick_duration = Duration::new(0, 1_000_000_000 / tick_rate);

        //let level = World::new(legion::WorldOptions::default());

        //asset_manager.add_container(Box::new(GltfContainer::load::<P>("/home/robin/Projekte/SourceRenderer/MetalRoughSpheresNoTextures.glb").unwrap()));
        //c_asset_manager.add_container(Box::new(GltfContainer::load::<P>("MetalRoughSpheresNoTextures.glb").unwrap()));

        #[cfg(target_os = "android")]
        asset_manager.add_container(Box::new(
            GltfContainer::<P>::load("bistro_sun.glb", false).unwrap(),
        ));

        #[cfg(target_os = "linux")]
        asset_manager.add_container(Box::new(
            GltfContainer::<P>::load("/home/robin/Projekte/bistro/bistro_sun.glb", true).unwrap(),
        ));

        #[cfg(target_os = "windows")]
        asset_manager.add_container(Box::new(
            GltfContainer::<P>::load("bistro_sun.glb", true)
                .unwrap(),
        ));

        //asset_manager.add_container(Box::new(GltfContainer::<P>::load("/home/robin/Projekte/SourceRenderer/assets/Sponza2/Sponza.glb", true).unwrap()));
        asset_manager.add_loader(Box::new(GltfLoader::new()));
        asset_manager.add_loader(Box::new(ImageLoader::new()));
        let mut level = asset_manager.load_level("bistro_sun.glb/scene/Scene").unwrap();
        //let mut level = asset_manager.load_level("Sponza.glb/scene/Scene").unwrap();
        //let mut level = asset_manager.load_level("MetalRoughSpheresNoTextures.glb/scene/0").unwrap();

        #[cfg(target_os = "linux")]
        let csgo_path =
            "/home/robin/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive";
        //let csgo_path = "/run/media/robin/System/Program Files (x86)/Steam/steamapps/common/Counter-Strike Global Offensive";
        #[cfg(target_os = "windows")]
        let csgo_path =
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive";
        #[cfg(target_os = "android")]
      let csgo_path = "content://com.android.externalstorage.documents/tree/primary%3Agames%2Fcsgo/document/primary%3Agames%2Fcsgo";
        #[cfg(target_arch = "wasm32")]
        let csgo_path = "";
        #[cfg(target_os = "macos")]
        let csgo_path = "";

        trace!("Csgo path: {:?}", csgo_path);

        /*let mut level = {
          asset_manager.add_container(Box::new(CSGODirectoryContainer::new::<P>(csgo_path).unwrap()));
          let progress = asset_manager.request_asset("pak01_dir", AssetType::Container, AssetLoadPriority::Normal);
          while !progress.is_done() {
            // wait until our container is loaded
          }
          asset_manager.load_level("de_overpass.bsp").unwrap()
        };*/
        trace!("Done loading level");
        /*let mut level = asset_manager
            .load_level("assets/helmet/FlightHelmet.gltf/scene/0")
            .unwrap();*/

        PhysicsWorld::install(
            &mut world,
            &mut resources,
            &mut fixed_schedule,
            tick_duration,
        );
        crate::spinning_cube::install(
            &mut world,
            &mut resources,
            &mut fixed_schedule,
            asset_manager,
        );
        fps_camera::install::<P>(&mut world, &mut fixed_schedule);
        transform::interpolation::install(&mut fixed_schedule, &mut schedule);
        transform::install(&mut fixed_schedule);
        renderer.install(&mut world, &mut resources, &mut schedule);

        let point_light_entity = world.push((
            Transform {
                position: Vec3::new(0f32, 0f32, 0f32),
                rotation: UnitQuaternion::default(),
                scale: Vec3::new(1f32, 1f32, 1f32),
            },
            PointLightComponent { intensity: 1.0f32 },
        ));

        trace!("Point Light: {:?}", point_light_entity);

        world.move_from(&mut level, &FilterAll {});

        //resources.insert(c_renderer.primary_camera().clone());

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
            tick_duration,
            window_event_receiver,
            ui
        }
    }

    pub fn update(&mut self, game: &Game<P>, renderer: &Arc<Renderer<P>>) {
        self.resources.insert(game.input().poll());

        let now = Instant::now();

        // run fixed step systems first
        let mut tick_delta = now.duration_since(self.last_tick_time);
        if renderer.is_saturated() && tick_delta < self.tick_duration {
            renderer.wait_until_available(self.tick_duration - tick_delta);
            return;
        }

        while tick_delta >= self.tick_duration {
            self.last_tick_time += self.tick_duration;
            self.resources.insert(Tick(self.tick));
            self.fixed_schedule
                .execute(&mut self.world, &mut self.resources);
            self.tick += 1;
            tick_delta = now.duration_since(self.last_tick_time);
        }

        let mut window_event_opt = self.window_event_receiver.try_recv();
        while window_event_opt.is_ok() {
            let window_event = window_event_opt.unwrap();

            match window_event {
                Event::FingerDown(_)
                | Event::FingerUp(_)
                | Event::FingerMoved { .. }
                | Event::MouseMoved(_)
                | Event::SurfaceChanged(_)
                | Event::WindowMinimized
                | Event::Quit
                | Event::KeyUp(_)
                | Event::KeyDown(_) => {},
                Event::WindowRestored(size) => {
                    self.ui.set_window_size(size);
                },
                Event::WindowSizeChanged(size) => {
                    self.ui.set_window_size(size);
                },
            }

            window_event_opt = self.window_event_receiver.try_recv();
        }
        if let Err(TryRecvError::Disconnected) = window_event_opt {
            panic!("Window event channel disconnected");
        }

        let delta = now.duration_since(self.last_iter_time);
        self.last_iter_time = now;
        self.resources.insert(TickDelta(tick_delta));
        self.resources.insert(DeltaTime(delta));
        self.schedule.execute(&mut self.world, &mut self.resources);

        self.ui.update();
        let ui_data = self.ui.draw_data(renderer.device());
        renderer.update_ui(ui_data);
    }
}
