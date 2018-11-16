use sourcerenderer_core::Platform;
use sourcerenderer_core::Window;
use sourcerenderer_core::PlatformEvent;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::Sdl;
use std::time::Duration;
use sdl2::VideoSubsystem;
use sdl2::EventPump;

pub struct SDLPlatform {
    sdl_context: Sdl,
    video_subsystem: VideoSubsystem,
    event_pump: EventPump,
    window: SDLWindow
}

pub struct SDLWindow {
    window: sdl2::video::Window
}

impl SDLPlatform {
    pub fn new() -> SDLPlatform {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        let window = SDLWindow::new(&sdl_context, &video_subsystem);
        let mut event_pump = sdl_context.event_pump().unwrap();

        return SDLPlatform {
            sdl_context: sdl_context,
            video_subsystem: video_subsystem,
            event_pump: event_pump,
            window: window
        };
    }
}

impl SDLWindow {    
    pub fn new(sdl_context: &Sdl, video_subsystem: &VideoSubsystem) -> SDLWindow {
        let window = video_subsystem.window("sourcerenderer", 1280, 720)
            .position_centered()
            .vulkan()
            .build()
            .unwrap();

        return SDLWindow {
            window: window
        };
    }
}

impl Platform for SDLPlatform {
    fn get_window(&mut self) -> &mut Window {
        return &mut self.window;
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
}

impl Window for SDLWindow {
}