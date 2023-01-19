use std::error::Error;
use std::sync::Arc;

use crate::{Vec2, Vec2I, Vec2UI, graphics::{self, Backend}};
use crate::input::Key;

mod io;
pub use io::IO;
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

pub trait ThreadHandle : Send + Sync {
  fn join(self) -> Result<(), Box<dyn std::any::Any + Send + 'static>>;
}

pub trait Platform: 'static + Sized {
  type GraphicsBackend: graphics::Backend + Send + Sync;
  type Window: Window<Self>;
  type IO: io::IO;
  type ThreadHandle: ThreadHandle;

  fn window(&self) -> &Self::Window;
  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<<Self::GraphicsBackend as graphics::Backend>::Instance>, Box<dyn Error>>;

  fn start_thread<F>(&self, name: &str, callback: F) -> Self::ThreadHandle
  where
      F: FnOnce(),
      F: Send + 'static;
}

#[derive(PartialEq)]
pub enum Event<P: Platform> {
  KeyDown(Key),
  KeyUp(Key),
  Quit,
  WindowMinimized,
  SurfaceChanged(Arc<<P::GraphicsBackend as Backend>::Surface>),
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

impl<P: Platform> Clone for Event<P> {
    fn clone(&self) -> Self {
        match self {
            Self::KeyDown(key) => Self::KeyDown(*key),
            Self::KeyUp(key) => Self::KeyUp(*key),
            Self::Quit => Self::Quit,
            Self::WindowMinimized => Self::WindowMinimized,
            Self::SurfaceChanged(surface) => Self::SurfaceChanged(surface.clone()),
            Self::WindowRestored(size) => Self::WindowRestored(*size),
            Self::WindowSizeChanged(size) => Self::WindowSizeChanged(*size),
            Self::MouseMoved(mouse_pos) => Self::MouseMoved(*mouse_pos),
            Self::FingerDown(finger_index) => Self::FingerDown(*finger_index),
            Self::FingerUp(finger_index) => Self::FingerUp(*finger_index),
            Self::FingerMoved { index, position } => Self::FingerMoved { index: *index, position: *position },
        }
    }
}

pub trait Window<P: Platform> {
  fn create_surface(&self, graphics_instance: Arc<<P::GraphicsBackend as graphics::Backend>::Instance>) -> Arc<<P::GraphicsBackend as graphics::Backend>::Surface>;
  fn create_swapchain(&self, vsync: bool, device: &<P::GraphicsBackend as graphics::Backend>::Device, surface: &Arc<<P::GraphicsBackend as graphics::Backend>::Surface>) -> Arc<<P::GraphicsBackend as graphics::Backend>::Swapchain>;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
