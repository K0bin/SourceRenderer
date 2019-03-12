use renderer::resource::Texture;
use renderer::resource::Mesh;
use platform::Window;
use renderer::Vertex;

pub trait Renderer {
  fn create_texture(&mut self) -> Box<Texture>;
  fn create_mesh(&mut self, vertex_size: u64, index_size: u64) -> Box<Mesh>;
  fn render(&mut self);
}
