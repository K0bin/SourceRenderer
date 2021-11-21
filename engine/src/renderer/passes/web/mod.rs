use std::sync::Arc;

use sourcerenderer_core::{Platform, graphics::{Backend, CommandBuffer, Device, LoadOp, Queue, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderpassRecordingMode, StoreOp, Swapchain}};

use crate::{input::Input, renderer::{LateLatching, render_path::RenderPath}};

mod geometry;

use self::geometry::GeometryPass;

pub struct WebRenderer<B: Backend> {
  device: Arc<B::Device>,
  swapchain: Arc<B::Swapchain>,
  geometry: GeometryPass<B>
}

impl<B: Backend> WebRenderer<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
    let geometry_pass = GeometryPass::new::<P>(device, swapchain, &mut init_cmd_buffer);
    let init_submission = init_cmd_buffer.finish();
    device.graphics_queue().submit(init_submission, None, &[], &[]);
    Self {
      device: device.clone(),
      swapchain: swapchain.clone(),
      geometry: geometry_pass
    }
  }
}

impl<B: Backend> RenderPath<B> for WebRenderer<B> {
  fn on_swapchain_changed(&mut self, _swapchain: &std::sync::Arc<B::Swapchain>) {
  }

  fn render(
    &mut self,
    scene: &std::sync::Arc<sourcerenderer_core::atomic_refcell::AtomicRefCell<crate::renderer::RendererScene<B>>>,
    view: &std::sync::Arc<sourcerenderer_core::atomic_refcell::AtomicRefCell<crate::renderer::View>>,
    _zero_texture_view: &Arc<B::TextureShaderResourceView>,
    _lightmap: &std::sync::Arc<crate::renderer::renderer_assets::RendererTexture<B>>,
    _late_latching: Option<&dyn LateLatching<B>>,
    _input: &Input
  ) -> Result<(), sourcerenderer_core::graphics::SwapchainError> {

    let queue = self.device.graphics_queue();
    let mut cmd_buffer = queue.create_command_buffer();

    let scene_ref = scene.borrow();
    let view_ref = view.borrow();
    let semaphore = self.geometry.execute(&mut cmd_buffer, &self.device, &scene_ref, &view_ref);
    let submit_semaphore = self.device.create_semaphore();
    queue.submit(cmd_buffer.finish(), None, &[&semaphore], &[&submit_semaphore]);
    queue.present(&self.swapchain, &[&submit_semaphore]);
    Ok(())
  }
}
