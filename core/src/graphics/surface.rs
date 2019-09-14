use std::sync::Arc;

use graphics::Queue;

pub trait Surface {

}

pub struct SwapchainInfo {
  pub width: u32,
  pub height: u32,
  pub vsync: bool
}

pub trait Swapchain {
  fn recreate(&mut self, info: SwapchainInfo);
  fn present(&self, queue: Arc<dyn Queue>);
}
