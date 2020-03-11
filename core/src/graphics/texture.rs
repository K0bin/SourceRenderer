use std::sync::Arc;

use graphics::Backend;

pub trait Texture : Send {

}

pub trait RenderTargetView<B: Backend> : Send {
  fn get_texture(&self) -> Arc<B::Texture>;
}
