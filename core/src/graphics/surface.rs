use std::sync::Arc;

use graphics::Queue;
use graphics::Texture;
use graphics::Semaphore;

pub trait Surface {

}

pub struct SwapchainInfo {
  pub width: u32,
  pub height: u32,
  pub vsync: bool
}

pub trait Swapchain {
  fn recreate(&mut self, info: SwapchainInfo);
  fn start_frame(&self, index: u32) -> (Arc<dyn Semaphore>, Arc<dyn Texture>);
  fn present(&self, queue: Arc<dyn Queue>);
}
