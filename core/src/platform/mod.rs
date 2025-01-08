use std::error::Error;
use std::marker::PhantomData;

use crate::{Vec2, Vec2I, Vec2UI, gpu::GPUBackend};
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
  type GPUBackend: GPUBackend;
  type Window: Window<Self>;
  type IO: io::IO;
  type ThreadHandle: ThreadHandle;

  fn window(&self) -> &Self::Window;
  fn create_graphics(&self, debug_layers: bool) -> Result<<Self::GPUBackend as GPUBackend>::Instance, Box<dyn Error>>;

  fn thread_memory_management_pool<F, T>(callback: F) -> T
    where F: FnOnce() -> T;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct PlatformPhantomData<P: Platform>(PhantomData<P>);
unsafe impl<P: Platform> Send for PlatformPhantomData<P> {}
unsafe impl<P: Platform> Sync for PlatformPhantomData<P> {}
impl<P: Platform> Default for PlatformPhantomData<P> { fn default() -> Self { Self(PhantomData) } }

#[derive(PartialEq)]
pub enum Event<P: Platform> {
  KeyDown(Key),
  KeyUp(Key),
  Quit,
  WindowMinimized,
  SurfaceChanged(<P::GPUBackend as GPUBackend>::Surface),
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

pub trait Window<P: Platform> {
  fn create_surface(&self, graphics_instance: &<P::GPUBackend as GPUBackend>::Instance) -> <P::GPUBackend as GPUBackend>::Surface;
  fn create_swapchain(&self, vsync: bool, device: &<P::GPUBackend as GPUBackend>::Device, surface: <P::GPUBackend as GPUBackend>::Surface) -> <P::GPUBackend as GPUBackend>::Swapchain;
  fn width(&self) -> u32;
  fn height(&self) -> u32;
}
