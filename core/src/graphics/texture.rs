use std::sync::Arc;

pub trait Texture {

}

pub trait RenderTargetView {
  fn get_texture(&self) -> Arc<Texture>;
}
