use std::collections::HashMap;
use std::error::Error;
use std::io::Result as IOResult;
use std::path::{
    Path,
    PathBuf,
};

use crossbeam_channel::Sender;
use log::debug;
use notify::{
    recommended_watcher,
    RecommendedWatcher,
    Watcher,
};
use sdl2::event::{
    Event as SDLEvent,
    WindowEvent,
};
use sdl2::keyboard::Scancode;
use sdl2::{
    EventPump,
    Sdl,
    VideoSubsystem,
};
use sourcerenderer_core::platform::{
    FileWatcher,
    Platform,
    ThreadHandle,
    Window,
    IO
};
use sourcerenderer_core::{
    Vec2I,
    Vec2UI,
    Vec2
};
use crate::sdl_gpu;
use sourcerenderer_engine::{Engine, WindowState};
use bevy_input::keyboard::{KeyboardInput, KeyCode, Key};
use bevy_input::ButtonState;
use bevy_input::mouse::MouseMotion;
use bevy_ecs::entity::Entity;

lazy_static! {
    pub static ref SCANCODE_TO_KEY: HashMap<Scancode, KeyCode> = {
        let mut key_to_scancode: HashMap<Scancode, KeyCode> = HashMap::new();
        key_to_scancode.insert(Scancode::W, KeyCode::KeyW);
        key_to_scancode.insert(Scancode::A, KeyCode::KeyA);
        key_to_scancode.insert(Scancode::S, KeyCode::KeyS);
        key_to_scancode.insert(Scancode::D, KeyCode::KeyD);
        key_to_scancode.insert(Scancode::Q, KeyCode::KeyQ);
        key_to_scancode.insert(Scancode::E, KeyCode::KeyE);
        key_to_scancode.insert(Scancode::Space, KeyCode::Space);
        key_to_scancode.insert(Scancode::LShift, KeyCode::ShiftLeft);
        key_to_scancode.insert(Scancode::LCtrl, KeyCode::ControlLeft);
        key_to_scancode.insert(Scancode::Escape, KeyCode::Escape);
        key_to_scancode
    };
}

pub struct SDLPlatform {
    sdl_context: Sdl,
    _video_subsystem: VideoSubsystem,
    event_pump: EventPump,
    window: SDLWindow,
    _mouse_pos: Vec2I,
}

pub struct SDLWindow {
    window: sdl2::video::Window,
    _is_active: bool,
}

impl SDLPlatform {
    pub fn new() -> Box<SDLPlatform> {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        let event_pump = sdl_context.event_pump().unwrap();

        let window = SDLWindow::new(&sdl_context, &video_subsystem);

        Box::new(SDLPlatform {
            sdl_context,
            _video_subsystem: video_subsystem,
            event_pump,
            window,
            _mouse_pos: Vec2I::new(0, 0),
        })
    }

    pub(crate) fn poll_events(&mut self, engine: &mut Engine) -> bool {
        let mut event_opt = self.event_pump.poll_event();
        while let Some(event) = event_opt {
            match event {
                SDLEvent::Quit { .. } => {
                    engine.stop::<SDLPlatform>();
                    return false;
                }
                SDLEvent::KeyUp {
                    scancode: Some(keycode),
                    ..
                } => {
                    let key = SCANCODE_TO_KEY.get(&keycode).copied();
                    if let Some(key) = key {
                        engine.dispatch_keyboard_input(KeyboardInput {
                            key_code: key,
                            logical_key: Key::Dead(None),
                            state: ButtonState::Released,
                            window: Entity::from_raw(0u32),
                            repeat: false
                        });
                    }
                }
                SDLEvent::KeyDown {
                    scancode: Some(keycode),
                    ..
                } => {
                    let key = SCANCODE_TO_KEY.get(&keycode).copied();
                    if let Some(key) = key {
                        engine.dispatch_keyboard_input(KeyboardInput {
                            key_code: key,
                            logical_key: Key::Dead(None),
                            state: ButtonState::Pressed,
                            window: Entity::from_raw(0u32),
                            repeat: false
                        });
                    }
                }
                SDLEvent::MouseMotion {
                    x: _x, y: _y, xrel, yrel, ..
                } => {
                    engine.dispatch_mouse_motion(MouseMotion {
                        delta: Vec2::new(xrel as f32, yrel as f32)
                    });
                }
                SDLEvent::Window {
                    window_id: _,
                    timestamp: _,
                    win_event,
                } => match win_event {
                    WindowEvent::Resized(width, height) => {
                        engine.window_changed::<SDLPlatform>(WindowState::Window(Vec2UI::new(
                            width as u32,
                            height as u32,
                        )));
                    }
                    WindowEvent::SizeChanged(width, height) => {
                        engine.window_changed::<SDLPlatform>(WindowState::Window(Vec2UI::new(
                            width as u32,
                            height as u32,
                        )));
                    }
                    WindowEvent::Close => {
                        engine.stop::<SDLPlatform>();
                    }
                    _ => {}
                },
                _ => {}
            }
            event_opt = self.event_pump.poll_event()
        }
        true
    }

    pub(crate) fn update_mouse_lock(&self, is_locked: bool) {
        let mouse_util = self.sdl_context.mouse();
        mouse_util.set_relative_mouse_mode(is_locked);
        if is_locked {
            let (width, height) = self.window.window.drawable_size();
            mouse_util.warp_mouse_in_window(self.window.sdl_window_handle(), width as i32 / 2, height as i32 / 2);
        }
    }
}

impl SDLWindow {
    pub fn new(
        _sdl_context: &Sdl,
        video_subsystem: &VideoSubsystem,
    ) -> SDLWindow {
        let mut window_builder = video_subsystem.window("sourcerenderer", 1920, 1080);
        window_builder.position_centered();
        //window_builder.fullscreen();

        sdl_gpu::prepare_window(&mut window_builder);

        let window = window_builder.build().unwrap();
        SDLWindow {
            window,
            _is_active: true,
        }
    }

    pub(crate) fn sdl_window_handle(&self) -> &sdl2::video::Window {
        &self.window
    }
}

impl Platform for SDLPlatform {
    type Window = SDLWindow;
    type GPUBackend = sdl_gpu::SDLGPUBackend;
    type IO = StdIO;
    type ThreadHandle = StdThreadHandle;

    fn window(&self) -> &SDLWindow {
        &self.window
    }

    fn create_graphics(&self, debug_layers: bool) -> Result<<Self::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Instance, Box<dyn Error>> {
        sdl_gpu::create_instance(debug_layers, &self.window)
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn thread_memory_management_pool<F, T>(callback: F) -> T
        where F: FnOnce() -> T {
            use objc::rc::autoreleasepool;
        autoreleasepool(callback)
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    fn thread_memory_management_pool<F, T>(callback: F) -> T
        where F: FnOnce() -> T {
        callback()
    }
}

impl Window<SDLPlatform> for SDLWindow {
    fn create_surface(&self, graphics_instance: &<<SDLPlatform as sourcerenderer_core::Platform>::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Instance) -> <<SDLPlatform as sourcerenderer_core::Platform>::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Surface {
        sdl_gpu::create_surface(&self.window, graphics_instance)
    }

    fn create_swapchain(
        &self,
        vsync: bool,
        device: &<<SDLPlatform as sourcerenderer_core::Platform>::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Device,
        surface: <<SDLPlatform as sourcerenderer_core::Platform>::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Surface
     ) -> <<SDLPlatform as sourcerenderer_core::Platform>::GPUBackend as sourcerenderer_core::gpu::GPUBackend>::Swapchain {
        let (width, height) = self.window.drawable_size();
        sdl_gpu::create_swapchain(vsync, width, height, device, surface)
    }

    fn width(&self) -> u32 {
        self.window.drawable_size().0
    }

    fn height(&self) -> u32 {
        self.window.drawable_size().1
    }
}

pub struct StdIO {}

impl IO for StdIO {
    type File = async_fs::File; // TODO: Replace with an implementation that uses Bevys IOTaskPool as executor
    type FileWatcher = NotifyFileWatcher;

    async fn open_asset<P: AsRef<Path> + Send>(path: P) -> IOResult<Self::File> {
        async_fs::File::open(path).await
    }

    async fn asset_exists<P: AsRef<Path> + Send>(path: P) -> bool {
        path.as_ref().exists()
    }

    async fn open_external_asset<P: AsRef<Path> + Send>(path: P) -> IOResult<Self::File> {
        async_fs::File::open(path).await
    }

    async fn external_asset_exists<P: AsRef<Path> + Send>(path: P) -> bool {
        path.as_ref().exists()
    }

    fn new_file_watcher(sender: Sender<String>) -> Self::FileWatcher {
        let base_path = std::env::current_dir().unwrap_or_else(|_e| PathBuf::new());
        NotifyFileWatcher::new(sender, &base_path)
    }
}

pub struct NotifyFileWatcher {
    watcher: RecommendedWatcher,
}

impl NotifyFileWatcher {
    fn new<P: AsRef<Path>>(sender: Sender<String>, base_path: &P) -> Self {
        let base_path = base_path.as_ref().to_str().unwrap().to_string();
        debug!("Working directory: {:?}", base_path);
        let watcher =
            recommended_watcher(
                move |event: Result<notify::Event, notify::Error>| match event {
                    Ok(event) => {
                        for path in event.paths {
                            let path_str = path.to_str().unwrap().to_string();
                            let relative_path = if path_str.starts_with(&base_path) {
                                &path_str[base_path.len() + 1..]
                            } else {
                                &path_str
                            };

                            sender.send(relative_path.to_string()).unwrap();
                        }
                    }
                    _ => {}
                },
            )
            .unwrap();
        Self { watcher }
    }
}

impl FileWatcher for NotifyFileWatcher {
    fn watch<P: AsRef<Path>>(&mut self, path: P) {
        self.watcher
            .watch(path.as_ref(), notify::RecursiveMode::NonRecursive)
            .unwrap();
    }

    fn unwatch<P: AsRef<Path>>(&mut self, path: P) {
        self.watcher.unwatch(path.as_ref()).unwrap();
    }
}

unsafe impl Send for NotifyFileWatcher {} // I'll just assume that the backends are Send even if the interface is not.

pub struct StdThreadHandle(std::thread::JoinHandle<()>);
impl ThreadHandle for StdThreadHandle {
    fn join(self) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {
        self.0.join()
    }
}
