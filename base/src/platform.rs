use std::error::Error;
use std::sync::Arc;

use crate::graphics::Instance;
use crate::graphics::Surface;
use crate::graphics::Device;
use crate::graphics::Swapchain;
use crate::graphics::SwapchainInfo;

use crate::graphics;

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

pub trait Platform<GB: graphics::Backend> {
  fn window(&mut self) -> &Window<GB>;
  fn handle_events(&mut self) -> PlatformEvent;
  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<GB::Instance>, Box<dyn Error>>;
}

pub trait Window<GB: graphics::Backend> {
  fn create_surface(&self, graphics_instance: Arc<GB::Instance>) -> Arc<GB::Surface>;
  fn create_swapchain(&self, info: SwapchainInfo, device: Arc<GB::Device>, surface: Arc<GB::Surface>) -> Arc<GB::Swapchain>;
}
