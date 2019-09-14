use std::error::Error;
use std::sync::Arc;

use crate::graphics::Instance;
use crate::graphics::Surface;
use crate::graphics::Adapter;
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
  fn window(&mut self) -> &Window;
  fn handle_events(&mut self) -> PlatformEvent;
  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<dyn Instance>, Box<Error>>;
}

pub trait Window {
  fn create_surface(&self, graphics_instance: Arc<Instance>) -> Arc<Surface>;
  fn create_swapchain(&self, info: SwapchainInfo, device: Arc<Device>, surface: Arc<Surface>) -> Arc<Swapchain>;
}
