use std::sync::Arc;

use sourcerenderer_core::{Platform, graphics::{Backend, CommandBuffer, Device, Queue, Swapchain}};

use crate::{input::Input, renderer::{LateLatching, render_path::{RenderPath, SceneInfo, ZeroTextures, FrameInfo}, renderer_resources::RendererResources}};

mod geometry;

use self::geometry::GeometryPass;

pub struct WebRenderer<B: Backend> {
  device: Arc<B::Device>,
  swapchain: Arc<B::Swapchain>,
  geometry: GeometryPass<B>,
  resources: RendererResources<B>,
}

impl<B: Backend> WebRenderer<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let mut resources = RendererResources::<B>::new(device);
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
    let geometry_pass = GeometryPass::new::<P>(device, swapchain, &mut init_cmd_buffer, &mut resources);
    let init_submission = init_cmd_buffer.finish();
    device.graphics_queue().submit(init_submission, None, &[], &[], false);
    Self {
      device: device.clone(),
      swapchain: swapchain.clone(),
      geometry: geometry_pass,
      resources,
    }
  }
}

impl<B: Backend> RenderPath<B> for WebRenderer<B> {
  fn write_occlusion_culling_results(&self, _frame: u64, bitset: &mut Vec<u32>) {
    bitset.fill(!0u32);
  }

  fn on_swapchain_changed(&mut self, _swapchain: &Arc<B::Swapchain>) {
  }

  fn render(
    &mut self,
    scene: &SceneInfo<B>,
    _zero_textures: &ZeroTextures<B>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    _frame_info: &FrameInfo,
  ) -> Result<(), sourcerenderer_core::graphics::SwapchainError> {


    let semaphore = self.device.create_semaphore();
    let backbuffer = self.swapchain.prepare_back_buffer(&semaphore).unwrap();

    let queue = self.device.graphics_queue();
    let mut cmd_buffer = queue.create_command_buffer();

    let view_ref = &scene.views[scene.active_view_index];
    let late_latching_buffer = late_latching.unwrap().buffer();
    self.geometry.execute(&mut cmd_buffer, scene.scene, &view_ref, &late_latching_buffer, &self.resources, &backbuffer);

    if let Some(late_latching) = late_latching {
      let input_state = input.poll();
      late_latching.before_submit(&input_state, &view_ref);
    }

    let submit_semaphore = self.device.create_semaphore();
    queue.submit(cmd_buffer.finish(), None, &[&semaphore], &[&submit_semaphore], false);
    queue.present(&self.swapchain, &[&submit_semaphore], false);

    if let Some(late_latching) = late_latching {
      late_latching.after_submit(&self.device);
    }

    Ok(())
  }
}
