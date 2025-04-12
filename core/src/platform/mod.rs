use std::error::Error;

use crate::{Vec2, Vec2I, Vec2UI, gpu::GPUBackend};
use crate::input::Key;

mod io;
pub use io::PlatformIO;
pub use io::FileWatcher;

#[derive(PartialEq)]
pub enum PlatformEvent {
  Continue,
  Quit
}

#[derive(PartialEq)]
#[derive(Copy)]
#[derive(Clone)]
pub enum GraphicsApi {
  OpenGLES,
  Vulkan
}

pub trait GraphicsPlatform<B: GPUBackend> : 'static {
  fn create_instance(debug_layers: bool) -> Result<B::Instance, Box<dyn Error>>;
}

#[derive(PartialEq)]
pub enum Event<B: GPUBackend> {
  KeyDown(Key),
  KeyUp(Key),
  Quit,
  WindowMinimized,
  SurfaceChanged(B::Surface),
  WindowRestored(Vec2UI),
  WindowSizeChanged(Vec2UI),
  MouseMoved(Vec2I),
  FingerDown(u32),
  FingerUp(u32),
  FingerMoved {
    index: u32,
    position: Vec2
  }
}

impl<B: GPUBackend> Clone for Event<B> {
    fn clone(&self) -> Self {
        match self {
            Self::KeyDown(key) => Self::KeyDown(*key),
            Self::KeyUp(key) => Self::KeyUp(*key),
            Self::Quit => Self::Quit,
            Self::WindowMinimized => Self::WindowMinimized,
            Self::SurfaceChanged(_) => panic!("Cannot clone surface changed event"),
            Self::WindowRestored(size) => Self::WindowRestored(*size),
            Self::WindowSizeChanged(size) => Self::WindowSizeChanged(*size),
            Self::MouseMoved(mouse_pos) => Self::MouseMoved(*mouse_pos),
            Self::FingerDown(finger_index) => Self::FingerDown(*finger_index),
            Self::FingerUp(finger_index) => Self::FingerUp(*finger_index),
            Self::FingerMoved { index, position } => Self::FingerMoved { index: *index, position: *position },
        }
    }
}

pub trait Window<B: GPUBackend> {
  fn width(&self) -> u32;
  fn height(&self) -> u32;
  fn create_surface(&self, graphics_instance: &B::Instance) -> B::Surface;
}
