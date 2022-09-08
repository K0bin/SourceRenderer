use std::sync::Arc;
use std::time::Duration;

use sourcerenderer_core::{graphics::{Backend, SwapchainError}, Platform};

use crate::input::Input;

use super::{LateLatching, drawable::View, renderer_assets::{RendererTexture, RendererAssets}, renderer_scene::RendererScene, shader_manager::ShaderManager};

pub struct SceneInfo<'a, B: Backend> {
  pub scene: &'a RendererScene<B>,
  pub views: &'a [View],
  pub active_view_index: usize,
  pub vertex_buffer: &'a Arc<B::Buffer>,
  pub index_buffer: &'a Arc<B::Buffer>,
  pub lightmap: Option<&'a RendererTexture<B>>,
}

pub struct ZeroTextures<'a, B: Backend> {
  pub zero_texture_view: &'a Arc<B::TextureSamplingView>,
  pub zero_texture_view_black: &'a Arc<B::TextureSamplingView>,
}

pub struct FrameInfo {
  pub frame: u64,
  pub delta: Duration
}

pub(super) trait RenderPath<P: Platform> {
  fn is_gpu_driven(&self) -> bool;
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
  fn on_swapchain_changed(&mut self, swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>);
  fn render(
    &mut self,
    scene: &SceneInfo<P::GraphicsBackend>,
    zero_textures: &ZeroTextures<P::GraphicsBackend>,
    late_latching: Option<&dyn LateLatching<P::GraphicsBackend>>,
    input: &Input,
    frame_info: &FrameInfo,
    shader_manager: &ShaderManager<P>,
    assets: &RendererAssets<P>
  ) -> Result<(), SwapchainError>;
}
