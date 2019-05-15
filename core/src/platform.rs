use std::error::Error;
use std::sync::Arc;

use crate::graphics::Instance;

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

pub trait Platform {
  fn window(&mut self) -> &Window;
  fn handle_events(&mut self) -> PlatformEvent;
  fn create_graphics(&self) -> Result<Arc<dyn Instance>, Box<Error>>;
}

pub trait Window {
}
