use std::collections::HashMap;
use std::error::Error;
use std::io::Result as IOResult;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use ash::extensions::khr::Surface as SurfaceLoader;
use ash::vk::{
    Handle,
    SurfaceKHR,
};
use crossbeam_channel::Sender;
use notify::{
    recommended_watcher,
    RecommendedWatcher,
    Watcher,
};
use sdl2::event::{
    Event as SDLEvent,
    WindowEvent,
};
use sdl2::keyboard::{
    Keycode,
    Scancode,
};
use sdl2::{
    EventPump,
    Sdl,
    VideoSubsystem,
};
use sourcerenderer_core::input::Key;
use sourcerenderer_core::platform::{
    Event,
    FileWatcher,
    GraphicsApi,
    Platform,
    ThreadHandle,
    Window,
    IO,
};
use sourcerenderer_core::{
    Vec2I,
    Vec2UI,
};
use sourcerenderer_engine::Engine;
use sourcerenderer_vulkan::{
    VkDevice,
    VkInstance,
    VkSurface,
    VkSwapchain,
};

use sourcerenderer_vulkan::new;

lazy_static! {
    pub static ref SCANCODE_TO_KEY: HashMap<Scancode, Key> = {
        let mut key_to_scancode: HashMap<Scancode, Key> = HashMap::new();
        key_to_scancode.insert(Scancode::W, Key::W);
        key_to_scancode.insert(Scancode::A, Key::A);
        key_to_scancode.insert(Scancode::S, Key::S);
        key_to_scancode.insert(Scancode::D, Key::D);
        key_to_scancode.insert(Scancode::Q, Key::Q);
        key_to_scancode.insert(Scancode::E, Key::E);
        key_to_scancode.insert(Scancode::Space, Key::Space);
        key_to_scancode.insert(Scancode::LShift, Key::LShift);
        key_to_scancode.insert(Scancode::LCtrl, Key::LCtrl);
        key_to_scancode
    };
}

pub struct SDLPlatform {
    sdl_context: Sdl,
    video_subsystem: VideoSubsystem,
    event_pump: EventPump,
    window: SDLWindow,
    mouse_pos: Vec2I,
}

pub struct SDLWindow {
    window: sdl2::video::Window,
    graphics_api: GraphicsApi,
    is_active: bool,
}

impl SDLPlatform {
    pub fn new(graphics_api: GraphicsApi) -> Box<SDLPlatform> {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        let event_pump = sdl_context.event_pump().unwrap();

        let window = SDLWindow::new(&sdl_context, &video_subsystem, graphics_api);

        Box::new(SDLPlatform {
            sdl_context,
            video_subsystem,
            event_pump,
            window,
            mouse_pos: Vec2I::new(0, 0),
        })
    }

    pub(crate) fn poll_events(&mut self, engine: &Engine<Self>) -> bool {
        let mut event_opt = Some(self.event_pump.wait_event());
        while let Some(event) = event_opt {
            match event {
                SDLEvent::Quit { .. }
                | SDLEvent::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    engine.dispatch_event(Event::Quit);
                    return false;
                }
                SDLEvent::KeyUp {
                    scancode: Some(keycode),
                    ..
                } => {
                    let key = SCANCODE_TO_KEY.get(&keycode).copied();
                    if let Some(key) = key {
                        engine.dispatch_event(Event::KeyUp(key));
                    }
                }
                SDLEvent::KeyDown {
                    scancode: Some(keycode),
                    ..
                } => {
                    let key = SCANCODE_TO_KEY.get(&keycode).copied();
                    if let Some(key) = key {
                        engine.dispatch_event(Event::KeyDown(key));
                    }
                }
                SDLEvent::MouseMotion {
                    x, y, xrel, yrel, ..
                } => {
                    if engine.is_mouse_locked() {
                        self.mouse_pos += Vec2I::new(xrel, yrel);
                        engine.dispatch_event(Event::MouseMoved(self.mouse_pos));
                    } else {
                        engine.dispatch_event(Event::MouseMoved(Vec2I::new(x, y)));
                    }
                }
                SDLEvent::Window {
                    window_id: _,
                    timestamp: _,
                    win_event,
                } => match win_event {
                    WindowEvent::Resized(width, height) => {
                        engine.dispatch_event(Event::WindowSizeChanged(Vec2UI::new(
                            width as u32,
                            height as u32,
                        )));
                    }
                    WindowEvent::SizeChanged(width, height) => {
                        engine.dispatch_event(Event::WindowSizeChanged(Vec2UI::new(
                            width as u32,
                            height as u32,
                        )));
                    }
                    WindowEvent::Close => {
                        engine.dispatch_event(Event::Quit);
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
        //mouse_util.set_relative_mouse_mode(is_locked);
        if is_locked {
            let (width, height) = self.window.window.drawable_size();
            //mouse_util.warp_mouse_in_window(self.window.sdl_window_handle(), width as i32 / 2, height as i32 / 2);
        }
    }
}

impl SDLWindow {
    pub fn new(
        _sdl_context: &Sdl,
        video_subsystem: &VideoSubsystem,
        graphics_api: GraphicsApi,
    ) -> SDLWindow {
        let mut window_builder = video_subsystem.window("sourcerenderer", 1920, 1080);
        window_builder.position_centered();
        //window_builder.fullscreen();

        match graphics_api {
            GraphicsApi::Vulkan => {
                window_builder.vulkan();
            }
            GraphicsApi::OpenGLES => {
                window_builder.opengl();
            }
        }

        let window = window_builder.build().unwrap();
        SDLWindow {
            graphics_api,
            window,
            is_active: true,
        }
    }

    pub(crate) fn sdl_window_handle(&self) -> &sdl2::video::Window {
        &self.window
    }

    #[inline]
    pub fn vulkan_instance_extensions(&self) -> Result<Vec<&str>, String> {
        self.window.vulkan_instance_extensions()
    }
}

impl Platform for SDLPlatform {
    type Window = SDLWindow;
    type GraphicsBackend = sourcerenderer_vulkan::VkBackend;
    type GPUBackend = sourcerenderer_vulkan::new::VkBackend;
    type IO = StdIO;
    type ThreadHandle = StdThreadHandle;

    fn window(&self) -> &SDLWindow {
        &self.window
    }

    fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<dyn Error>> {
        let extensions = self.window.vulkan_instance_extensions().unwrap();
        Ok(Arc::new(VkInstance::new(&extensions, debug_layers)))
    }

    fn create_graphics_new(&self, debug_layers: bool) -> Result<Arc<new::VkInstance>, Box<dyn Error>> {
        let extensions = self.window.vulkan_instance_extensions().unwrap();
        Ok(Arc::new(new::VkInstance::new(&extensions, debug_layers)))
    }

    fn start_thread<F>(&self, name: &str, callback: F) -> Self::ThreadHandle
    where
        F: FnOnce(),
        F: Send + 'static,
    {
        StdThreadHandle(
            std::thread::Builder::new()
                .name(name.to_string())
                .spawn(callback)
                .unwrap(),
        )
    }
}

impl Window<SDLPlatform> for SDLWindow {
    fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
        let instance_raw = graphics_instance.raw();
        let surface = self
            .window
            .vulkan_create_surface(
                instance_raw.instance.handle().as_raw() as sdl2::video::VkInstance
            )
            .unwrap();
        let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
        Arc::new(VkSurface::new(
            instance_raw,
            SurfaceKHR::from_raw(surface),
            surface_loader,
        ))
    }

    fn create_swapchain(
        &self,
        vsync: bool,
        device: &VkDevice,
        surface: &Arc<VkSurface>,
    ) -> Arc<VkSwapchain> {
        let device_inner = device.inner();
        let (width, height) = self.window.drawable_size();
        VkSwapchain::new(
            vsync,
            width,
            height,
            device_inner,
            surface,
            device.graphics_queue(),
            device.compute_queue(),
            device.transfer_queue(),
        )
        .unwrap()
    }

    fn create_surface_new(&self, graphics_instance: &Arc<<P::GPUBackend as GPUBackend>::Instance>) -> <P::GPUBackend as GPUBackend>::Surface {
        let instance_raw = graphics_instance.raw();
        let surface = self
            .window
            .vulkan_create_surface(
                instance_raw.instance.handle().as_raw() as sdl2::video::VkInstance
            )
            .unwrap();
        let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
        sourcerenderer_vulkan::new::VkSurface::new(
            graphics_instance,
            SurfaceKHR::from_raw(surface),
            surface_loader
        )
    }

    fn create_swapchain_new(
        &self,
        vsync: bool,
        device: &<P::GPUBackend as GPUBackend>::Device,
        surface: <P::GPUBackend as GPUBackend>::Surface
     ) -> <P::GPUBackend as GPUBackend>::Swapchain {
        let device_inner = device.inner();
        let (width, height) = self.window.drawable_size();
        sourcerenderer_vulkan::new::VkSwapchain::new(
            vsync,
            width,
            height,
            device_inner,
            surface
        )
        .unwrap()
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
    type File = std::fs::File;
    type FileWatcher = NotifyFileWatcher;

    fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
        std::fs::File::open(path)
    }

    fn asset_exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
        std::fs::File::open(path)
    }

    fn external_asset_exists<P: AsRef<Path>>(path: P) -> bool {
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
        println!("base path: {:?}", base_path);
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
