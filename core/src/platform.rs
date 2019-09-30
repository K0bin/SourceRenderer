use std::error::Error;
use std::sync::Arc;

use crate::graphics::Instance;
use crate::graphics::Surface;
use crate::graphics::Device;
use crate::graphics::Swapchain;
use crate::graphics::SwapchainInfo;

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
  fn window(&mut self) -> &dyn Window;
  fn handle_events(&mut self) -> PlatformEvent;
  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<dyn Instance>, Box<dyn Error>>;
}

pub trait Window {
  fn create_surface(&self, graphics_instance: Arc<dyn Instance>) -> Arc<dyn Surface>;
  fn create_swapchain(&self, info: SwapchainInfo, device: Arc<dyn Device>, surface: Arc<dyn Surface>) -> Arc<dyn Swapchain>;
}
