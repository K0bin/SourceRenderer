use std::error::Error;
use std::sync::Arc;

use crate::graphics;

mod input;
pub use input::Input;
pub use input::Key;

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

pub trait Platform: 'static + Sized {
  type GraphicsBackend: graphics::Backend + Send + Sync;
  type Window: Window<Self>;
  type Input: Input;

  fn input(&self) -> &Arc<Self::Input>;
  fn window(&mut self) -> &Self::Window;
  fn handle_events(&mut self) -> PlatformEvent;
  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<<Self::GraphicsBackend as graphics::Backend>::Instance>, Box<dyn Error>>;
}

#[derive(Clone)]
pub enum WindowState {
  Minimized,
  Visible {
    width: u32,
    height: u32,
    focussed: bool
  },
  FullScreen {
    width: u32,
    height: u32
  }
}

pub trait Window<P: Platform> {
  fn create_surface(&self, graphics_instance: Arc<<P::GraphicsBackend as graphics::Backend>::Instance>) -> Arc<<P::GraphicsBackend as graphics::Backend>::Surface>;
  fn create_swapchain(&self, vsync: bool, device: &<P::GraphicsBackend as graphics::Backend>::Device, surface: &Arc<<P::GraphicsBackend as graphics::Backend>::Surface>) -> Arc<<P::GraphicsBackend as graphics::Backend>::Swapchain>;
  fn state(&self) -> WindowState;
}
