use platform::{Platform, PlatformEvent, GraphicsApi};

use std::time::Duration;

pub struct Engine {
    platform: Box<Platform>
}

impl Engine {
    pub fn new(mut platform: Box<Platform>) -> Engine {
        return Engine {
            platform: platform
        };
    }

    pub fn run(&mut self) {

        let renderer = self.platform.create_renderer();
        'main_loop: loop {
            let event = self.platform.handle_events();
            if event == PlatformEvent::Quit {
                break 'main_loop;
            }
            //renderer.render();
            std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
        }
    }
}