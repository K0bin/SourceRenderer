use renderer::{Renderer};
use std::error::Error;

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
  fn create_renderer(&self) -> Result<Box<Renderer>, Box<Error>>;
}

pub trait Window {
}
