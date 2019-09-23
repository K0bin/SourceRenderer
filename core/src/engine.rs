use platform::{Platform, PlatformEvent, GraphicsApi};
use job::{Scheduler, JobThreadContext};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use graphics::SwapchainInfo;
use graphics::QueueType;
use graphics::CommandBufferType;
use graphics::CommandBuffer;
use std::rc::Rc;

pub struct Engine {
    platform: Box<Platform>,
    scheduler: Arc<Mutex<Scheduler>>
}

pub trait EngineSubsystem {
  fn init_contexts() -> Vec<Box<dyn JobThreadContext>>;
}

impl Engine {
  pub fn new(mut platform: Box<Platform>) -> Engine {
    return Engine {
      platform: platform,
      scheduler: Scheduler::new(0)
    };
  }

  pub fn run(&mut self) {
    self.init();
    //let renderer = self.platform.create_renderer();
    let graphics = self.platform.create_graphics(true).unwrap();
    let surface = self.platform.window().create_surface(graphics.clone());

    let mut adapters = graphics.list_adapters();
    println!("n devices: {}", adapters.len());

    let device = adapters.remove(0).create_device(surface.clone());
    let swapchain_info = SwapchainInfo {
      width: 1920,
      height: 1080,
      vsync: true
    };
    let swapchain = self.platform.window().create_swapchain(swapchain_info, device.clone(), surface.clone());
    let queue = device.create_queue(QueueType::Graphics).unwrap();
    let mut tracker: Vec<Rc<dyn CommandBuffer>> = Vec::new();
    {
    let command_pool = queue.create_command_pool();
    let command_buffer = command_pool.clone().create_command_buffer(CommandBufferType::PRIMARY);
    }

    'main_loop: loop {
      let event = self.platform.handle_events();
      if event == PlatformEvent::Quit {
          break 'main_loop;
      }
      //renderer.render();
      std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
    }
    tracker.pop();
  }

  fn init(&mut self) {

  }
}