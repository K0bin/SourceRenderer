use std::sync::Arc;
use std::time::Duration;

use sourcerenderer_core::graphics::{Backend, SwapchainError};

use crate::input::Input;

use super::{LateLatching, drawable::View, renderer_assets::RendererTexture, renderer_scene::RendererScene};

pub struct SceneInfo<'a, B: Backend> {
  pub scene: &'a RendererScene<B>,
  pub views: &'a [View],
  pub active_view_index: usize,
  pub vertex_buffer: &'a Arc<B::Buffer>,
  pub index_buffer: &'a Arc<B::Buffer>,
  pub lightmap: Option<&'a Arc<RendererTexture<B>>>,
}

pub struct ZeroTextures<'a, B: Backend> {
  pub zero_texture_view: &'a Arc<B::TextureSamplingView>,
  pub zero_texture_view_black: &'a Arc<B::TextureSamplingView>,
}

pub struct FrameInfo {
  pub frame: u64,
  pub delta: Duration
}

pub struct CommonResourceNames<'a> {
  pub motion_vectors: Option<&'a str>,
  pub depth: Option<&'a str>,
  pub pre_postprocessing_output: &'a str,
  pub pre_upscaling_output: &'a str,
  pub output: &'a str,
}

pub(super) trait RenderPath<B: Backend> {
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
  fn on_swapchain_changed(&mut self, swapchain: &Arc<B::Swapchain>);
  fn render(
    &mut self,
    scene: &SceneInfo<B>,
    zero_textures: &ZeroTextures<B>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame_info: &FrameInfo,
  ) -> Result<(), SwapchainError>;
}
