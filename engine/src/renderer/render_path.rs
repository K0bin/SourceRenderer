use std::sync::Arc;
use std::time::Duration;

use sourcerenderer_core::{atomic_refcell::AtomicRefCell, graphics::{Backend, SwapchainError}};

use crate::input::Input;

use super::{LateLatching, drawable::View, renderer_assets::RendererTexture, renderer_scene::RendererScene};

pub(super) trait RenderPath<B: Backend> {
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
  fn on_swapchain_changed(&mut self, swapchain: &Arc<B::Swapchain>);
  fn render(
    &mut self,
    scene: &AtomicRefCell<RendererScene<B>>,
    view: &AtomicRefCell<View>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    zero_texture_view_black: &Arc<B::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame: u64,
    delta: Duration,
    vertex_buffer: &Arc<B::Buffer>,
    index_buffer: &Arc<B::Buffer>
  ) -> Result<(), SwapchainError>;
}
