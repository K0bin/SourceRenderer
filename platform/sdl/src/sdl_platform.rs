use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::io::Result as IOResult;
use std::path::Path;

use sourcerenderer_core::platform::io::IO;
use sourcerenderer_core::{Vec2I, Vec2UI};
use sourcerenderer_core::input::Key;
use sourcerenderer_core::platform::{Event, Platform};

use sourcerenderer_core::platform::Window;
use sourcerenderer_core::platform::GraphicsApi;

use sourcerenderer_engine::Engine;
use sourcerenderer_vulkan::VkInstance;
use sourcerenderer_vulkan::VkSurface;
use sourcerenderer_vulkan::VkSwapchain;
use sourcerenderer_vulkan::VkDevice;

use sdl2::event::{Event as SDLEvent, WindowEvent};
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::Sdl;
use sdl2::VideoSubsystem;
use sdl2::EventPump;

use ash::vk::{Handle, SurfaceKHR};
use ash::extensions::khr::Surface as SurfaceLoader;

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
  mouse_pos: Vec2I
}

pub struct SDLWindow {
  window: sdl2::video::Window,
  graphics_api: GraphicsApi,
  is_active: bool
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
      mouse_pos: Vec2I::new(0, 0)
    })
  }

  pub(crate) fn poll_events(&mut self, engine: &Engine<Self>) -> bool {
    let mut event_opt = Some(self.event_pump.wait_event());
    while let Some(event) = event_opt {
      match event {
        SDLEvent::Quit {..} |
        SDLEvent::KeyDown { keycode: Some(Keycode::Escape), .. } => {
          engine.dispatch_event(Event::Quit);
          return false;
        }
        SDLEvent::KeyUp { scancode: Some(keycode), .. } => {
          let key = SCANCODE_TO_KEY.get(&keycode).copied();
          if let Some(key) = key {
            engine.dispatch_event(Event::KeyUp(key));
          }
        }
        SDLEvent::KeyDown { scancode: Some(keycode), .. } => {
          let key = SCANCODE_TO_KEY.get(&keycode).copied();
          if let Some(key) = key {
            engine.dispatch_event(Event::KeyDown(key));
          }
        }
        SDLEvent::MouseMotion { x, y, .. } => {
          if engine.is_mouse_locked() {
            let (width, height) = self.window.window.drawable_size();
            if x - width as i32 / 2i32 != 0 || y - height as i32 / 2i32 != 0 {
              self.mouse_pos += Vec2I::new(x - width as i32 / 2i32, y - height as i32 / 2i32);
              engine.dispatch_event(Event::MouseMoved(self.mouse_pos));
            }
          } else {
            engine.dispatch_event(Event::MouseMoved(Vec2I::new(x, y)));
          }
        }
        SDLEvent::Window {
          window_id: _,
          timestamp: _,
          win_event
        } => {
          match win_event {
            WindowEvent::Resized(width, height) => {
              engine.dispatch_event(Event::WindowSizeChanged(Vec2UI::new(width as u32, height as u32)));
            }
            WindowEvent::SizeChanged(width, height) => {
              engine.dispatch_event(Event::WindowSizeChanged(Vec2UI::new(width as u32, height as u32)));
            },
            WindowEvent::Close => {
              engine.dispatch_event(Event::Quit);
            },
            _ => {}
          }
        }
        _ => {}
      }
      event_opt = self.event_pump.poll_event()
    }
    true
  }

  pub(crate) fn reset_mouse_position(&self) {
    let mouse_util = self.sdl_context.mouse();
    let (width, height) = self.window.sdl_window_handle().drawable_size();
    mouse_util.warp_mouse_in_window(self.window.sdl_window_handle(), width as i32 / 2, height as i32 / 2);
  }
}

impl SDLWindow {
  pub fn new(_sdl_context: &Sdl, video_subsystem: &VideoSubsystem, graphics_api: GraphicsApi) -> SDLWindow {
    let mut window_builder = video_subsystem.window("sourcerenderer", 1280, 720);
    window_builder.position_centered();

    match graphics_api {
      GraphicsApi::Vulkan => { window_builder.vulkan(); },
      GraphicsApi::OpenGLES => { window_builder.opengl(); },
    }

    let window = window_builder.build().unwrap();
    SDLWindow {
      graphics_api,
      window,
      is_active: true
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
  type IO = StdIO;

  fn window(&self) -> &SDLWindow {
    &self.window
  }

  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<dyn Error>> {
    let extensions = self.window.vulkan_instance_extensions().unwrap();
    Ok(Arc::new(VkInstance::new(&extensions, debug_layers)))
  }

  fn start_thread<F>(&self, name: &str, callback: F)
  where
        F: FnOnce(),
        F: Send + 'static {
    std::thread::Builder::new()
      .name(name.to_string())
      .spawn(callback)
      .unwrap();
  }
}

impl Window<SDLPlatform> for SDLWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    let instance_raw = graphics_instance.get_raw();
    let surface = self.window.vulkan_create_surface(instance_raw.instance.handle().as_raw() as sdl2::video::VkInstance).unwrap();
    let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
    Arc::new(VkSurface::new(instance_raw, SurfaceKHR::from_raw(surface), surface_loader))
  }

  fn create_swapchain(&self, vsync: bool, device: &VkDevice, surface: &Arc<VkSurface>) -> Arc<VkSwapchain> {
    let device_inner = device.get_inner();
    let (width, height) = self.window.drawable_size();
    VkSwapchain::new(vsync, width, height, device_inner, surface).unwrap()
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
}
