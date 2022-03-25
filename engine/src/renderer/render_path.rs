use std::sync::Arc;

use sourcerenderer_core::{atomic_refcell::AtomicRefCell, graphics::{Backend, SwapchainError}};

use crate::input::Input;

use super::{LateLatching, drawable::View, renderer_assets::RendererTexture, renderer_scene::RendererScene};

pub(super) trait RenderPath<B: Backend> {
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
  fn on_swapchain_changed(&mut self, swapchain: &Arc<B::Swapchain>);
  fn render(
    &mut self,
    scene: &Arc<AtomicRefCell<RendererScene<B>>>,
    view: &Arc<AtomicRefCell<View>>,
    zero_texture_view: &Arc<B::TextureShaderResourceView>,
    zero_texture_view_black: &Arc<B::TextureShaderResourceView>,
    lightmap: &Arc<RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame: u64
  ) -> Result<(), SwapchainError>;
}
