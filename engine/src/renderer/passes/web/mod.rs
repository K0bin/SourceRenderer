use std::sync::Arc;

use sourcerenderer_core::{Platform, graphics::Backend};

use crate::{input::Input, renderer::{LateLatching, render_path::RenderPath}};

pub struct WebRenderer<B: Backend> {
  device: Arc<B::Device>
}

impl<B: Backend> WebRenderer<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    Self {
      device: device.clone()
    }
  }
}

impl<B: Backend> RenderPath<B> for WebRenderer<B> {
  fn on_swapchain_changed(&mut self, swapchain: &std::sync::Arc<B::Swapchain>) {
  }

  fn render(
    &mut self,
    scene: &std::sync::Arc<sourcerenderer_core::atomic_refcell::AtomicRefCell<crate::renderer::RendererScene<B>>>,
    view: &std::sync::Arc<sourcerenderer_core::atomic_refcell::AtomicRefCell<crate::renderer::View>>,
    lightmap: &std::sync::Arc<crate::renderer::renderer_assets::RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input
  ) -> Result<(), sourcerenderer_core::graphics::SwapchainError> {
    Ok(())
  }
}
