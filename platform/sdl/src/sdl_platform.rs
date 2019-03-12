use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::platform::Window;
use sourcerenderer_core::platform::PlatformEvent;
use sourcerenderer_core::platform::GraphicsApi;
use sourcerenderer_core::renderer::Renderer;
use sourcerenderer_vulkan::Renderer as VkRenderer;
use sourcerenderer_vulkan::initialize_vulkan;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::Sdl;
use sdl2::VideoSubsystem;
use sdl2::EventPump;
use sdl2::video::VkInstance;

use ash::version::InstanceV1_0;
use ash::vk::{Handle, SurfaceKHR};

use std::error::Error;

pub struct SDLPlatform {
  sdl_context: Sdl,
  video_subsystem: VideoSubsystem,
  event_pump: EventPump,
  window: SDLWindow
}

pub struct SDLWindow {
  window: sdl2::video::Window,
  graphics_api: GraphicsApi
}

impl SDLPlatform {
  pub fn new(graphics_api: GraphicsApi) -> SDLPlatform {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();

    let window = SDLWindow::new(&sdl_context, &video_subsystem, graphics_api);

    return SDLPlatform {
      sdl_context: sdl_context,
      video_subsystem: video_subsystem,
      event_pump: event_pump,
      window: window
    };
  }
}

impl SDLWindow {
  pub fn new(sdl_context: &Sdl, video_subsystem: &VideoSubsystem, graphics_api: GraphicsApi) -> SDLWindow {
    let mut window_builder = video_subsystem.window("sourcerenderer", 1280, 720);
    window_builder.position_centered();

    match graphics_api {
      GraphicsApi::Vulkan => { window_builder.vulkan(); },
      GraphicsApi::OpenGLES => { window_builder.opengl(); },
    }

    let window = window_builder.build().unwrap();
    return SDLWindow {
      graphics_api: graphics_api,
      window: window
    };
  }

  fn create_renderer(&self) -> Result<Box<dyn Renderer>, Box<Error>> {
    return match self.graphics_api {
      GraphicsApi::Vulkan => {
        unsafe {
          let extensions = self.window.vulkan_instance_extensions().unwrap();
          let (entry, instance) = initialize_vulkan(extensions, true).unwrap();
          let surface = self.window.vulkan_create_surface(instance.handle().as_raw() as VkInstance).unwrap();
          let renderer = VkRenderer::new(entry, instance, SurfaceKHR::from_raw(surface)).unwrap();
          Ok(renderer)
        }
      }

      GraphicsApi::OpenGLES => {
        panic!("ogl");
      }
    }
  }
}

impl Platform for SDLPlatform {
  fn window(&mut self) -> &Window {
    return &self.window;
  }

  fn handle_events(&mut self) -> PlatformEvent {
    for event in self.event_pump.poll_iter() {
      match event {
        Event::Quit {..} |
        Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
          return PlatformEvent::Quit;
        },
        _ => {}
      }
    }
    return PlatformEvent::Continue;
  }

  fn create_renderer(&self) -> Result<Box<dyn Renderer>, Box<Error>> {
    return self.window.create_renderer();
  }
}

impl Window for SDLWindow {
}
