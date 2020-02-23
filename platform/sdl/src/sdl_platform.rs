use std::error::Error;
use std::sync::Arc;

use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::platform::Window;
use sourcerenderer_core::platform::PlatformEvent;
use sourcerenderer_core::platform::GraphicsApi;
use sourcerenderer_core::graphics::Instance;
use sourcerenderer_core::graphics::Surface;
use sourcerenderer_core::graphics::Device;
use sourcerenderer_core::graphics::Swapchain;
use sourcerenderer_core::graphics::SwapchainInfo;

use sourcerenderer_vulkan::VkInstance;
use sourcerenderer_vulkan::VkSurface;
use sourcerenderer_vulkan::VkSwapchain;
use sourcerenderer_vulkan::VkDevice;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::Sdl;
use sdl2::VideoSubsystem;
use sdl2::EventPump;
use sdl2::video::VkInstance as SdlVkInstance;

use ash::version::InstanceV1_0;
use ash::vk::{Handle, SurfaceKHR};
use ash::extensions::khr::Surface as SurfaceLoader;

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

  #[inline]
  pub fn vulkan_instance_extensions(&self) -> Result<Vec<&str>, String> {
    return self.window.vulkan_instance_extensions();
  }
}

impl Platform for SDLPlatform {
  type Window = SDLWindow;
  type GraphicsBackend = sourcerenderer_vulkan::VkBackend;

  fn window(&mut self) -> &SDLWindow {
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

  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<Error>> {
    let extensions = self.window.vulkan_instance_extensions().unwrap();
    return Ok(Arc::new(VkInstance::new(extensions, debug_layers)));
  }
}

impl Window<SDLPlatform> for SDLWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    let instance_raw = graphics_instance.get_raw();
    let surface = self.window.vulkan_create_surface(instance_raw.instance.handle().as_raw() as sdl2::video::VkInstance).unwrap();
    let surface_loader = SurfaceLoader::new(&instance_raw.entry, &instance_raw.instance);
    return Arc::new(VkSurface::new(instance_raw, SurfaceKHR::from_raw(surface), surface_loader));
  }

  fn create_swapchain(&self, info: SwapchainInfo, device: &VkDevice, surface: &Arc<VkSurface>) -> VkSwapchain {
    return VkSwapchain::new(info, device, surface);
  }
}
