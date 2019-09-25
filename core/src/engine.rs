use cgmath::Vector3;
use platform::{Platform, PlatformEvent, GraphicsApi};
use job::{Scheduler, JobThreadContext};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use graphics::SwapchainInfo;
use graphics::QueueType;
use graphics::CommandBufferType;
use graphics::CommandBuffer;
use graphics::MemoryUsage;
use graphics::BufferUsage;
use std::rc::Rc;

pub struct Engine {
    platform: Box<Platform>,
    scheduler: Arc<Mutex<Scheduler>>
}

pub trait EngineSubsystem {
  fn init_contexts() -> Vec<Box<dyn JobThreadContext>>;
}

struct Vertex {
  pub position: Vector3<f32>
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
    let queue = device.clone().create_queue(QueueType::Graphics).unwrap();
    let mut tracker: Vec<Rc<dyn CommandBuffer>> = Vec::new();
    {
    let command_pool = queue.create_command_pool();
    let command_buffer = command_pool.clone().create_command_buffer(CommandBufferType::PRIMARY);
    }

    let buffer = device.create_buffer(8096, MemoryUsage::CpuOnly, BufferUsage::VERTEX);
    let triangle = [
      Vertex {
        position: Vector3 {
          x: 0.0f32,
          y: 0.0f32,
          z: 0.0f32,
        }
      },
      Vertex {
        position: Vector3 {
          x: 1.0f32,
          y: 0.0f32,
          z: 0.0f32,
        }
      },
      Vertex {
        position: Vector3 {
          x: 0.0f32,
          y: 1.0f32,
          z: 0.0f32,
        }
      }
    ];
    let ptr = buffer.map().expect("failed to map buffer");
    unsafe {
      std::ptr::copy(triangle.as_ptr(), ptr as *mut Vertex, 3);
    }
    buffer.unmap();

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