use std::sync::Arc;

use graphics::Texture;

use graphics::Backend;

pub trait Surface {

}

pub struct SwapchainInfo {
  pub width: u32,
  pub height: u32,
  pub vsync: bool
}

pub trait Swapchain {
}
