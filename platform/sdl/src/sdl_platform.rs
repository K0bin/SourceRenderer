use std::error::Error;
use std::sync::Arc;

use sdl2::mouse::MouseUtil;
use sourcerenderer_core::{Vec2I, Vec2UI};
use sourcerenderer_core::platform::{Event, InputCommands, InputState, Platform};

use sourcerenderer_core::platform::Window;
use sourcerenderer_core::platform::PlatformEvent;
use sourcerenderer_core::platform::GraphicsApi;
use sourcerenderer_core::platform::WindowState;

use sourcerenderer_engine::Engine;
use sourcerenderer_vulkan::VkInstance;
use sourcerenderer_vulkan::VkSurface;
use sourcerenderer_vulkan::VkSwapchain;
use sourcerenderer_vulkan::VkDevice;

use sdl2::event::{Event as SDLEvent, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::Sdl;
use sdl2::VideoSubsystem;
use sdl2::EventPump;

use sdl2_sys::SDL_WindowFlags;

use ash::vk::{Handle, SurfaceKHR};
use ash::extensions::khr::Surface as SurfaceLoader;

use crate::input;

pub struct SDLPlatform {
  sdl_context: Sdl,
  video_subsystem: VideoSubsystem,
  event_pump: EventPump,
  window: SDLWindow,
  input_commands: InputCommands,
  input_state: InputState
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
      input_commands: InputCommands::default(),
      input_state: InputState::default()
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
          engine.dispatch_event(Event::KeyUp(input::SCANCODE_TO_KEY.get(&keycode).copied().unwrap()));
        }
        SDLEvent::KeyDown { scancode: Some(keycode), .. } => {
          engine.dispatch_event(Event::KeyDown(input::SCANCODE_TO_KEY.get(&keycode).copied().unwrap()));
        }
        SDLEvent::MouseMotion { x, y, .. } => {
          engine.dispatch_event(Event::MouseMoved(Vec2I::new(x, y)));
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

  pub(crate) fn process_input(&mut self, input_commands: InputCommands) {
    self.input_state = crate::input::process(&mut self.input_commands, input_commands, &self.event_pump, &self.sdl_context.mouse(), &self.window);
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
  type IO = crate::io::StdIO;

  fn window(&self) -> &SDLWindow {
    &self.window
  }

  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<dyn Error>> {
    let extensions = self.window.vulkan_instance_extensions().unwrap();
    Ok(Arc::new(VkInstance::new(&extensions, debug_layers)))
  }

  fn input_state(&self) -> InputState {
    self.input_state.clone()
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

  fn state(&self) -> WindowState {
    if !self.is_active {
      return WindowState::Exited;
    }

    let (width, height) = self.window.drawable_size();
    let flags = self.window.window_flags();
    let fullscreen = (flags & SDL_WindowFlags::SDL_WINDOW_FULLSCREEN as u32) != 0 || (flags & SDL_WindowFlags::SDL_WINDOW_FULLSCREEN_DESKTOP as u32) != 0;
    let minimized = width == 0 || height == 0 || (flags & SDL_WindowFlags::SDL_WINDOW_MINIMIZED as u32) != 0 || (flags & SDL_WindowFlags::SDL_WINDOW_HIDDEN as u32) != 0;
    let focussed = (flags & SDL_WindowFlags::SDL_WINDOW_INPUT_FOCUS as u32) != 0 || (flags & SDL_WindowFlags::SDL_WINDOW_INPUT_GRABBED as u32) != 0;
    if minimized {
      WindowState::Minimized
    } else if fullscreen {
      WindowState::FullScreen {
        width,
        height
      }
    } else {
      WindowState::Visible {
        width,
        height,
        focussed
      }
    }
  }
}
