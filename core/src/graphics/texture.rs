use std::sync::Arc;

use graphics::Backend;

pub trait Texture {

}

pub trait RenderTargetView<B: Backend> {
  fn get_texture(&self) -> Arc<B::Texture>;
}
