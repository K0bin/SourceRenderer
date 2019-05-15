use platform::{Platform, PlatformEvent, GraphicsApi};
use job::{Scheduler, JobThreadContext};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
      let graphics = self.platform.create_graphics().unwrap();
      let adapters = graphics.list_adapters();

      println!("n devices: {}", adapters.len());

      for device in adapters {
        println!("Device");
        println!("device: {:?}", device.adapter_type());
      }

      'main_loop: loop {
        let event = self.platform.handle_events();
        if event == PlatformEvent::Quit {
            break 'main_loop;
        }
        //renderer.render();
        std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
      }
    }

    fn init(&mut self) {

    }
}