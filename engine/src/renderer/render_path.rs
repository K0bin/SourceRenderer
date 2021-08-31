use std::sync::Arc;

use sourcerenderer_core::{atomic_refcell::AtomicRefCell, graphics::{Backend, SwapchainError}};

use super::{LateLatchCamera, drawable::View, renderer_assets::RendererTexture, renderer_scene::RendererScene};

pub(super) trait RenderPath<B: Backend> {
  fn on_swapchain_changed(&mut self, swapchain: &Arc<B::Swapchain>);
  fn render(
    &mut self,
    scene: &Arc<AtomicRefCell<RendererScene<B>>>,
    view: &Arc<AtomicRefCell<View>>,
    lightmap: &Arc<RendererTexture<B>>,
    primary_camera: &Arc<LateLatchCamera<B>>
  ) -> Result<(), SwapchainError>;
}
