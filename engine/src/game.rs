use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use crossbeam_channel::{unbounded, Sender};
use instant::Instant;
use legion::query::{
    FilterResult,
    LayoutFilter,
};
use legion::storage::ComponentTypeId;
use log::trace;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::platform::{ThreadHandle, Event, Window};
use sourcerenderer_core::{Platform, Vec2UI};

use crate::asset::loaders::{
    BspLevelLoader,
    FSContainer,
    GltfLoader,
    MDLModelLoader,
    VMTMaterialLoader,
    VPKContainerLoader,
    VTFTextureLoader,
};
use crate::asset::AssetManager;
use crate::fps_camera::FPSCamera;
use crate::game_internal::GameInternal;
use crate::input::{
    Input,
    InputState,
};
use crate::renderer::*;

pub struct TimeStampedInputState(InputState, Instant);

enum GameImpl<P: Platform> {
    MultiThreaded(P::ThreadHandle),
    SingleThreaded(Box<GameInternal<P>>),
    Uninitialized,
}

unsafe impl<P: Platform> Send for GameImpl<P> {}
unsafe impl<P: Platform> Sync for GameImpl<P> {}

#[cfg(feature = "threading")]
pub struct Game<P: Platform> {
    input: Arc<Input>,
    fps_camera: Mutex<FPSCamera>,
    is_running: AtomicBool,
    window_event_sender: Sender<Event<P>>,
    game_impl: AtomicRefCell<GameImpl<P>>,
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
        tick_rate: u32,
    ) -> Arc<Self> {
        asset_manager.add_loader(Box::new(BspLevelLoader::new()));
        asset_manager.add_loader(Box::new(VPKContainerLoader::new()));
        asset_manager.add_loader(Box::new(VTFTextureLoader::new()));
        asset_manager.add_loader(Box::new(VMTMaterialLoader::new()));
        asset_manager.add_loader(Box::new(MDLModelLoader::new()));
        asset_manager.add_loader(Box::new(GltfLoader::new()));

        //asset_manager.add_container(Box::new(FSContainer::new(platform, &asset_manager)));

        let (window_sender, window_receiver) = unbounded();

        let window = platform.window();
        let window_size = Vec2UI::new(window.width(), window.height());

        let game = Arc::new(Self {
            input: input.clone(),
            fps_camera: Mutex::new(FPSCamera::new()),
            is_running: AtomicBool::new(true),
            game_impl: AtomicRefCell::new(GameImpl::Uninitialized),
            window_event_sender: window_sender
        });

        let c_renderer = renderer.clone();
        let c_asset_manager = asset_manager.clone();
        let c_game = Arc::downgrade(&game);
        if cfg!(feature = "threading") {
            let thread_handle = platform.start_thread("GameThread", move || {
                trace!("Started game thread");
                let game = c_game.upgrade().unwrap();
                let mut internal = GameInternal::new(&c_asset_manager, &c_renderer, tick_rate, window_receiver, window_size);
                loop {
                    if !game.is_running() {
                        break;
                    }
                    internal.update(&game, &c_renderer);
                }
                game.is_running.store(false, Ordering::SeqCst);
                trace!("Stopped game thread");
            });
            {
                let mut thread_handle_guard = game.game_impl.borrow_mut();
                *thread_handle_guard = GameImpl::MultiThreaded(thread_handle);
            }
        } else {
            let internal = GameInternal::new(&c_asset_manager, &c_renderer, tick_rate, window_receiver, window_size);
            let mut thread_handle_guard = game.game_impl.borrow_mut();
            *thread_handle_guard = GameImpl::SingleThreaded(Box::new(internal));
        }

        game
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        trace!("Stopping game");
        if cfg!(feature = "threading") {
            let was_running = self.is_running.swap(false, Ordering::SeqCst);
            if !was_running {
                return;
            }

            let mut game_impl = self.game_impl.borrow_mut();

            if let GameImpl::Uninitialized = &*game_impl {
                return;
            }

            let game_impl = std::mem::replace(&mut *game_impl, GameImpl::Uninitialized);

            match game_impl {
                GameImpl::MultiThreaded(thread_handle) => {
                    if let Err(e) = thread_handle.join() {
                        log::error!("Game thread did not exit cleanly: {:?}", e);
                    }
                }
                GameImpl::Uninitialized => {
                    panic!("Game was already stopped.");
                }
                _ => {}
            }
        }
    }

    pub fn input(&self) -> &Input {
        self.input.as_ref()
    }

    pub fn update(&self, renderer: &Arc<Renderer<P>>) {
        let mut game_impl = self.game_impl.borrow_mut();
        if let GameImpl::SingleThreaded(game) = &mut *game_impl {
            game.update(self, renderer);
        }
    }

    pub fn dispatch_window_event(&self, event: Event<P>) {
        self.window_event_sender.send(event).unwrap();
    }
}
