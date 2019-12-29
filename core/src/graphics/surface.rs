use std::sync::Arc;

use graphics::Queue;
use graphics::Texture;
use graphics::Semaphore;

use graphics::Backend;

pub trait Surface<B: Backend> {

}

pub struct SwapchainInfo {
  pub width: u32,
  pub height: u32,
  pub vsync: bool
}

pub trait Swapchain<B: Backend> {
  fn recreate(&mut self, info: SwapchainInfo);
  fn start_frame(&self, index: u32) -> (Arc<dyn Semaphore>, Arc<B::Texture>);
  fn present(&self, queue: Arc<B::Queue>);
}
